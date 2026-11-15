import { useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ApiError, api, upload } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Loading, Modal, Notice, PageHeader, Section } from "../components";
import { useLoad } from "../hooks";
import { fmtDate, fmtDateTime } from "../labels";

const proposalLabel: Record<string, string> = {
  creation: "Création",
  mise_a_jour: "Mise à jour",
  inchange: "Inchangée",
  a_revoir: "À revoir",
  rejet: "Rejetée",
};
const proposalTone: Record<string, string> = {
  creation: "info",
  mise_a_jour: "en_cours",
  inchange: "annule",
  a_revoir: "a_faire",
  rejet: "echec",
};
const resultLabel: Record<string, string> = {
  cree: "Créée",
  mis_a_jour: "Mise à jour",
  inchange: "Inchangée",
  ignore: "Non appliquée",
  conflit: "Conflit",
  rejete: "Rejetée",
};

/** Envoi d'un classeur : aperçu sans écriture métier. */
export function ImportStartPage() {
  const { has } = useAuth();
  const navigate = useNavigate();
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);
  const batches = useLoad(() => (has("RH", "ADMIN") ? api<any[]>("/personnel-imports") : Promise.resolve([])), []);
  if (!has("RH", "ADMIN")) return <Notice kind="danger">Réservé au RH et à l'administration.</Notice>;

  const send = async (file: File) => {
    setBusy(true);
    setError(null);
    try {
      const r = await upload<any>("/personnel-imports", file);
      navigate(`/personnel/import/${r.batch.id}`);
    } catch (e) {
      setError(e as ApiError);
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };

  return (
    <>
      <PageHeader
        title="Importer le personnel (Excel)"
        subtitle="Reprise du personnel existant : aucune arrivée, tâche ni e-mail n'est créé."
        actions={
          <a className="btn" href="/api/v1/personnel/template.xlsx">
            Télécharger le modèle
          </a>
        }
      />
      <div className="grid-2">
        <Section title="1. Choisir le fichier">
          <ul className="small">
            <li>Utilisez le modèle : onglet « Personnel », en-têtes en ligne 1.</li>
            <li>ID salarié et Matricule sont du texte (les zéros initiaux sont conservés). Le matricule est facultatif.</li>
            <li>Nouvelle fiche : Prénom, Nom, Poste, Site et Date d'arrivée obligatoires. Mise à jour : une case vide conserve la valeur actuelle.</li>
            <li>Pas de formule, de salaire, de pièce RH ni de mot de passe.</li>
          </ul>
          <button className="btn btn-primary" disabled={busy} onClick={() => input.current?.click()}>
            {busy ? "Analyse…" : "Choisir un fichier .xlsx"}
          </button>
          <input
            ref={input}
            type="file"
            accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            hidden
            onChange={(e) => e.target.files?.[0] && send(e.target.files[0])}
          />
          <ErrorBox error={error} />
          <p className="small muted">L'étape suivante est un aperçu : rien n'est modifié tant que vous n'avez pas confirmé.</p>
        </Section>
        <Section title="Imports récents">
          {batches.data?.length === 0 ? (
            <Empty>Aucun import.</Empty>
          ) : (
            <ul className="timeline">
              {batches.data?.map((b) => (
                <li key={b.id}>
                  <Link to={`/personnel/import/${b.id}`}>{b.file_name}</Link> ·{" "}
                  <Badge status={b.status === "applique" ? "termine" : "a_faire"} label={b.status === "applique" ? "Appliqué" : "Aperçu"} />
                  <div className="small muted">
                    {fmtDateTime(b.created_at)} · {b.created_by}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Section>
      </div>
    </>
  );
}

/** Aperçu ligne par ligne, décisions du RH, confirmation explicite. */
export function ImportBatchPage() {
  const { id = "" } = useParams();
  const { has } = useAuth();
  const batch = useLoad(() => api<any>(`/personnel-imports/${id}`), [id]);
  const [error, setError] = useState<ApiError | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  if (!has("RH", "ADMIN")) return <Notice kind="danger">Réservé au RH et à l'administration.</Notice>;
  if (batch.loading && !batch.data) return <Loading />;
  if (batch.error) return <ErrorBox error={batch.error} onRetry={batch.reload} />;
  const { batch: b, rows } = batch.data;
  const applied = b.status === "applique";

  const decide = async (row: number, decision: string | null, employee_id?: string) => {
    setError(null);
    try {
      await api(`/personnel-imports/${id}/rows/${row}/decision`, { body: { decision, employee_id } });
      batch.reload();
    } catch (e) {
      setError(e as ApiError);
    }
  };

  const effective = (r: any): string => {
    if (r.proposal === "rejet") return "rejet";
    if (r.decision === "exclure") return "exclu";
    if (r.decision === "creer") return "creation";
    if (r.decision === "rattacher") return "mise_a_jour";
    return r.proposal;
  };
  const plan = rows.reduce((acc: Record<string, number>, r: any) => {
    const e = effective(r);
    acc[e] = (acc[e] ?? 0) + 1;
    return acc;
  }, {});
  const unresolved = rows.filter((r: any) => r.proposal === "a_revoir" && !r.decision).length;

  const commit = async () => {
    setBusy(true);
    setError(null);
    try {
      await api(`/personnel-imports/${id}/commit`, { body: {} });
      setConfirming(false);
      batch.reload();
    } catch (e) {
      setError(e as ApiError);
      setConfirming(false);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader
        title={`Import : ${b.file_name}`}
        subtitle={`${applied ? "Appliqué" : "Aperçu"} · ${rows.length} ligne(s) · déposé par ${b.created_by} le ${fmtDateTime(b.created_at)}`}
        actions={
          <Link className="btn" to="/personnel/import">
            Autre fichier
          </Link>
        }
      />
      {b.already_applied && !applied && (
        <Notice kind="danger">
          Ce classeur a déjà été importé le {fmtDateTime(b.already_applied.committed_at)} : il ne peut pas être appliqué une seconde fois.
        </Notice>
      )}
      <div className="stat-row">
        {[
          ["creation", "Créations"],
          ["mise_a_jour", "Mises à jour"],
          ["inchange", "Inchangées"],
          ["a_revoir", "À revoir"],
          ["exclu", "Exclues"],
          ["rejet", "Rejetées"],
        ].map(([k, l]) => (
          <div className="stat" key={k}>
            <div className="value">{plan[k] ?? 0}</div>
            <div className="label">{l}</div>
          </div>
        ))}
      </div>
      <ErrorBox error={error} />
      {applied ? (
        <Notice kind="ok">
          Import appliqué le {fmtDateTime(b.committed_at)} : {Object.entries(b.summary ?? {}).map(([k, n]) => `${n} ${resultLabel[k]?.toLowerCase() ?? k}`).join(", ")}.
          Aucune arrivée, tâche ni e-mail n'a été créé.
        </Notice>
      ) : (
        <div className="card row" style={{ justifyContent: "space-between", marginBottom: "1rem" }}>
          <span>
            {unresolved > 0
              ? `${unresolved} ligne(s) à revoir sans décision : elles ne seront pas appliquées.`
              : "Toutes les lignes ambiguës ont une décision."}{" "}
            Les lignes rejetées sont à corriger dans le fichier, puis à réimporter.
          </span>
          <button
            className="btn btn-primary"
            disabled={!!b.already_applied || ((plan.creation ?? 0) + (plan.mise_a_jour ?? 0) === 0)}
            onClick={() => setConfirming(true)}
          >
            Appliquer {(plan.creation ?? 0) + (plan.mise_a_jour ?? 0)} changement(s)…
          </button>
        </div>
      )}

      <Section title="Lignes du fichier">
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Ligne</th>
                <th>Proposition</th>
                <th>Personne</th>
                <th>Détail</th>
                <th>{applied ? "Résultat" : "Décision"}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r: any) => (
                <tr key={r.row_number}>
                  <td className="num">{r.row_number}</td>
                  <td>
                    <Badge status={proposalTone[r.proposal]} label={proposalLabel[r.proposal]} />
                  </td>
                  <td dir="auto">
                    <strong>
                      {r.raw.first_name ?? "—"} {r.raw.last_name ?? ""}
                    </strong>
                    <div className="small muted">
                      {r.raw.matricule && <span className="mono">{r.raw.matricule}</span>} {r.raw.job_title} {r.raw.site && `· ${r.raw.site}`}{" "}
                      {r.raw.arrival && `· ${fmtDate(r.raw.arrival)}`}
                    </div>
                  </td>
                  <td className="small">
                    {r.messages.map((m: any, i: number) => (
                      <div key={i} style={{ color: m.niveau === "erreur" ? "var(--danger)" : m.niveau === "attention" ? "var(--warn)" : undefined }}>
                        {m.texte}
                      </div>
                    ))}
                    {Object.entries(r.changes ?? {}).map(([field, c]: [string, any]) => (
                      <div key={field}>
                        {field} : <span className="muted">{String(c.avant ?? "—")}</span> → <strong>{String(c.apres)}</strong>
                      </div>
                    ))}
                  </td>
                  <td>
                    {applied ? (
                      <>
                        <Badge status={r.result === "cree" || r.result === "mis_a_jour" ? "termine" : r.result === "conflit" ? "echec" : "annule"} label={resultLabel[r.result] ?? "—"} />
                        {r.result_message && <div className="small muted">{r.result_message}</div>}
                        {r.result_employee_id && (
                          <div className="small">
                            <Link to={`/personnel/${r.result_employee_id}`}>Ouvrir la fiche</Link>
                          </div>
                        )}
                      </>
                    ) : r.proposal === "rejet" ? (
                      <span className="small muted">À corriger dans le fichier</span>
                    ) : (
                      <select
                        aria-label={`Décision ligne ${r.row_number}`}
                        value={r.decision === "rattacher" ? `rattacher:${r.decision_employee_id}` : r.decision ?? ""}
                        onChange={(e) => {
                          const v = e.target.value;
                          if (!v) decide(r.row_number, null);
                          else if (v.startsWith("rattacher:")) decide(r.row_number, "rattacher", v.slice(10));
                          else decide(r.row_number, v);
                        }}
                      >
                        <option value="">{r.proposal === "a_revoir" ? "Décider…" : `Appliquer (${proposalLabel[r.proposal].toLowerCase()})`}</option>
                        {r.proposal === "a_revoir" && !r.employee_id && <option value="creer">Nouvelle personne (homonyme)</option>}
                        {r.proposal === "a_revoir" &&
                          r.candidates.map((c: any) => (
                            <option key={c.id} value={`rattacher:${c.id}`}>
                              Rattacher à {c.nom} {c.matricule ? `(${c.matricule})` : ""} — {c.poste ?? "?"}, {c.site ?? "?"}
                            </option>
                          ))}
                        <option value="exclure">Exclure cette ligne</option>
                      </select>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Section>

      {confirming && (
        <Modal title="Confirmer l'import" onClose={() => setConfirming(false)}>
          <p>
            Vous allez appliquer <strong>{plan.creation ?? 0} création(s)</strong> et <strong>{plan.mise_a_jour ?? 0} mise(s) à jour</strong>.
            {unresolved > 0 && ` ${unresolved} ligne(s) à revoir sans décision seront ignorées.`} Les lignes rejetées ou exclues ne sont pas appliquées.
          </p>
          <p className="small muted">Aucune arrivée, tâche ni e-mail n'est créé. L'opération est tracée.</p>
          <div className="form-actions">
            <button className="btn" onClick={() => setConfirming(false)}>
              Annuler
            </button>
            <button className="btn btn-primary" disabled={busy} onClick={commit}>
              {busy ? "Application…" : "Appliquer maintenant"}
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
