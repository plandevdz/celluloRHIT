import { FormEvent, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api, ApiError, upload } from "../api";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, Notice, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import {
  archiveStatusLabel,
  DEPARTMENTS,
  fmtDateTime,
  needLabel,
  preferenceLabel,
  repriseProposalLabel,
  repriseResultLabel,
} from "../labels";

const PROFILE_NEEDS = ["pc", "telephone", "sim", "email_pro", "xena", "intelix", "carte_visite", "vehicule"];

// ------------------------------------------------------------------ profils de poste

export function ProfilesTab() {
  const list = useLoad(() => api<any[]>("/job-profiles"), []);
  const [editing, setEditing] = useState<any | null>(null);
  return (
    <Section title="Profils de poste" actions={<button className="btn btn-primary" onClick={() => setEditing({})}>+ Profil</button>}>
      <p className="small muted">Un profil suggère des besoins à l'annonce d'une arrivée et sert de cible aux mutations. Le modifier ne change aucun dossier existant.</p>
      {list.loading && !list.data ? (
        <Loading />
      ) : list.data?.length === 0 ? (
        <Empty>Aucun profil.</Empty>
      ) : (
        <table>
          <tbody>
            {list.data?.map((p) => (
              <tr key={p.id}>
                <td>
                  <strong>{p.name}</strong> {!p.active && <span className="tag">Inactif</span>}
                  <div className="small muted">
                    {p.department ?? "—"}
                    {p.field_based && " · terrain"}
                  </div>
                </td>
                <td className="small">
                  {p.needs.map((n: any) => needLabel[n.category] + (n.preference ? ` (${preferenceLabel[n.preference]?.toLowerCase()})` : "")).join(", ") || "—"}
                  {p.trainings.length > 0 && <div className="muted">Formations : {p.trainings.map((t: any) => t.name).join(", ")}</div>}
                </td>
                <td>
                  <button className="btn btn-small" onClick={() => setEditing(p)}>
                    Modifier
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {editing && <ProfileDialog profile={editing.id ? editing : null} onClose={() => setEditing(null)} onDone={() => { setEditing(null); list.reload(); }} />}
    </Section>
  );
}

function ProfileDialog({ profile, onClose, onDone }: { profile: any | null; onClose: () => void; onDone: () => void }) {
  const catalog = useLoad(() => api<any[]>("/trainings/catalog"), []);
  const [f, setF] = useState({
    name: profile?.name ?? "",
    department: profile?.department ?? "",
    description: profile?.description ?? "",
    typical_titles: profile?.typical_titles ?? "",
    access_notes: profile?.access_notes ?? "",
    field_based: !!profile?.field_based,
    active: profile?.active ?? true,
  });
  const [needs, setNeeds] = useState<Record<string, { on: boolean; preference: string }>>(() =>
    Object.fromEntries(
      PROFILE_NEEDS.map((c) => {
        const n = profile?.needs.find((x: any) => x.category === c);
        return [c, { on: !!n, preference: n?.preference ?? "" }];
      }),
    ),
  );
  const [trainings, setTrainings] = useState<string[]>(profile?.trainings.map((t: any) => t.catalog_id) ?? []);
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const body = {
      ...f,
      version: profile?.version,
      department: f.department || null,
      description: f.description || null,
      typical_titles: f.typical_titles || null,
      access_notes: f.access_notes || null,
      needs: Object.entries(needs)
        .filter(([, v]) => v.on)
        .map(([c, v]) => ({ category: c, preference: c === "pc" ? v.preference || null : null })),
      trainings,
    };
    try {
      await api(profile ? `/job-profiles/${profile.id}` : "/job-profiles", { method: profile ? "PUT" : "POST", body });
      onDone();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={profile ? `Profil — ${profile.name}` : "Nouveau profil de poste"} onClose={onClose} wide>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="Intitulé" required>
            <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required />
          </Field>
          <Field label="Département">
            <input list="departements-profil" value={f.department} onChange={(e) => setF({ ...f, department: e.target.value })} />
            <datalist id="departements-profil">
              {DEPARTMENTS.map((d) => (
                <option key={d} value={d} />
              ))}
            </datalist>
          </Field>
        </div>
        <fieldset>
          <legend>Besoins suggérés</legend>
          {PROFILE_NEEDS.map((c) => (
            <div key={c} className="row" style={{ marginBottom: ".3rem" }}>
              <label className="inline-choice">
                <input type="checkbox" checked={needs[c].on} onChange={(e) => setNeeds({ ...needs, [c]: { ...needs[c], on: e.target.checked } })} />
                {needLabel[c]}
              </label>
              {c === "pc" && needs.pc.on && (
                <select value={needs.pc.preference} onChange={(e) => setNeeds({ ...needs, pc: { ...needs.pc, preference: e.target.value } })} aria-label="Préférence de PC">
                  <option value="">Préférence…</option>
                  <option value="fixe">Fixe</option>
                  <option value="portable">Portable</option>
                  <option value="sans_preference">Sans préférence</option>
                </select>
              )}
            </div>
          ))}
        </fieldset>
        <fieldset>
          <legend>Formations requises</legend>
          <div className="inline-choice">
            {catalog.data?.filter((t) => t.active).map((t) => (
              <label key={t.id}>
                <input type="checkbox" checked={trainings.includes(t.id)} onChange={(e) => setTrainings(e.target.checked ? [...trainings, t.id] : trainings.filter((x) => x !== t.id))} />
                {t.name}
              </label>
            ))}
          </div>
        </fieldset>
        <Field label="Fonctions types">
          <input value={f.typical_titles} onChange={(e) => setF({ ...f, typical_titles: e.target.value })} />
        </Field>
        <Field label="Accès à créer (note pour l'IT)">
          <textarea rows={2} value={f.access_notes} onChange={(e) => setF({ ...f, access_notes: e.target.value })} />
        </Field>
        <Field label="Description">
          <textarea rows={2} value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
        </Field>
        <div className="row">
          <label className="inline-choice">
            <input type="checkbox" checked={f.field_based} onChange={(e) => setF({ ...f, field_based: e.target.checked })} /> Poste terrain
          </label>
          {profile && (
            <label className="inline-choice">
              <input type="checkbox" checked={f.active} onChange={(e) => setF({ ...f, active: e.target.checked })} /> Actif
            </label>
          )}
        </div>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            Enregistrer
          </button>
        </div>
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------------ wilayas

export function WilayasTab() {
  const list = useLoad(() => api<any[]>("/wilayas"), []);
  const [f, setF] = useState({ code: "", name: "", region: "" });
  const cmd = useCommand();
  return (
    <div className="grid-2">
      <Section title={`Wilayas (${list.data?.length ?? 0})`}>
        {list.data?.length === 0 ? (
          <Empty>Aucune wilaya : ajoutez-les ou reprenez la liste SharePoint « Wilayas ».</Empty>
        ) : (
          <table>
            <tbody>
              {list.data?.map((w) => (
                <tr key={w.id}>
                  <td className="num">{w.code}</td>
                  <td>{w.name}</td>
                  <td className="small muted">{w.region ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
      <Section title="Ajouter une wilaya">
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            const r = await cmd.run(() => api("/wilayas", { body: { code: Number(f.code), name: f.name, region: f.region || null } }));
            if (r) {
              setF({ code: "", name: "", region: "" });
              list.reload();
            }
          }}
        >
          <div className="form-grid">
            <Field label="Code" required>
              <input type="number" min={1} value={f.code} onChange={(e) => setF({ ...f, code: e.target.value })} required />
            </Field>
            <Field label="Nom" required>
              <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required dir="auto" />
            </Field>
            <Field label="Région">
              <input value={f.region} onChange={(e) => setF({ ...f, region: e.target.value })} />
            </Field>
          </div>
          <ErrorBox error={cmd.error} />
          <button className="btn btn-primary" disabled={cmd.busy}>
            Ajouter
          </button>
        </form>
      </Section>
    </div>
  );
}

// ------------------------------------------------------------------ archivage et paramètres

export function ArchiveTab() {
  const v = useLoad(() => api<any>("/admin/archive"), []);
  const settings = useLoad(() => api<any>("/admin/settings"), []);
  const cmd = useCommand();
  if (!v.data || !settings.data) return <Loading />;
  const s = settings.data;
  return (
    <div className="stack">
      <Section title="Paramètres d'exploitation">
        <dl className="kv">
          <dt>Envoi des avis</dt>
          <dd>
            {s.mail_mode === "graph" ? (
              <>
                Exchange Online (Microsoft Graph) depuis <strong>{s.mail_sender}</strong>
              </>
            ) : (
              <>
                <strong>Mode journal (test)</strong> : aucun e-mail n'est envoyé
              </>
            )}
          </dd>
          {s.mail_test_recipient && (
            <>
              <dt>Redirection de test</dt>
              <dd>tous les avis partent vers {s.mail_test_recipient}</dd>
            </>
          )}
          {s.mail_allowed_domains.length > 0 && (
            <>
              <dt>Destinataires pilotes</dt>
              <dd>domaines autorisés : {s.mail_allowed_domains.join(", ")}</dd>
            </>
          )}
          <dt>Archivage</dt>
          <dd>{s.archive_mode === "sharepoint" ? `SharePoint, dossier « ${s.archive_root} »` : "désactivé (copie locale sauvegardée)"}</dd>
          <dt>Alertes d'échéance</dt>
          <dd>
            {s.alert_days} jours avant · {s.alert_km} km avant un entretien · relance coupure après {s.escalation_minutes} min
          </dd>
          <dt>Version</dt>
          <dd>{s.version}</dd>
        </dl>
        <p className="small muted">Ces paramètres se règlent dans le fichier .env du serveur (aucun secret n'est affiché ici).</p>
      </Section>
      <Section title="Archivage des décharges et scans">
        <div className="state-strip">
          {Object.entries(v.data.counts as Record<string, number>).map(([k, n]) => (
            <Badge key={k} status={k} label={`${archiveStatusLabel[k] ?? k} : ${n}`} />
          ))}
        </div>
        <ErrorBox error={cmd.error} />
        {v.data.items.length === 0 ? (
          <Empty>Aucun document en attente ou en échec.</Empty>
        ) : (
          <table>
            <tbody>
              {v.data.items.map((x: any) => (
                <tr key={x.id}>
                  <td>
                    {x.movement} · {x.file_kind === "scan" ? `scan v${x.version}` : "original"}
                    <div className="small mono">{x.path}</div>
                  </td>
                  <td>
                    <Badge status={x.status} label={archiveStatusLabel[x.status]} />
                    <div className="small muted">
                      {x.attempts} essai(s){x.next_at && ` · prochain ${fmtDateTime(x.next_at)}`}
                    </div>
                    {x.error && <div className="small" style={{ color: "var(--danger)" }}>{x.error}</div>}
                  </td>
                  <td>
                    {x.status === "echec" && (
                      <button
                        className="btn btn-small"
                        onClick={async () => {
                          await cmd.run(() => api(`/admin/archive/${x.id}/retry`, { body: {} }));
                          v.reload();
                        }}
                      >
                        Relancer
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
    </div>
  );
}

// ------------------------------------------------------------------ reprise SharePoint

export function RepriseTab() {
  const lists = useLoad(() => api<any[]>("/reprise/lists"), []);
  const batches = useLoad(() => api<any[]>("/reprise/batches"), []);
  const [list, setList] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);
  const navigate = useNavigate();
  const current = lists.data?.find((l) => l.name === list);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!file || !list) return;
    setBusy(true);
    setError(null);
    try {
      const r = await upload<{ id: string }>(`/reprise/${list}/preview`, file);
      navigate(`/administration/reprise/${r.id}`);
    } catch (err) {
      setError(err as ApiError);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="stack">
      <Section title="Reprise des listes SharePoint">
        <Notice kind="info">
          Exportez chaque liste depuis SharePoint (« Exporter » → CSV, ou classeur Excel), puis chargez-la ici <strong>dans l'ordre proposé</strong> :
          les références d'abord, le personnel, puis le parc et les affectations. Chaque fichier passe par un aperçu sans écriture ; les
          lignes ambiguës attendent votre décision ; aucune arrivée, tâche ni avis n'est créé. Incluez la colonne <strong>ID</strong> si possible :
          elle conserve les liens et évite les doublons au rejeu.
        </Notice>
        <form onSubmit={submit}>
          <div className="form-grid">
            <Field label="Liste" required>
              <select value={list} onChange={(e) => setList(e.target.value)} required>
                <option value="">Choisir…</option>
                {lists.data?.map((l, i) => (
                  <option key={l.name} value={l.name}>
                    {i + 1}. {l.label} ({l.name})
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Fichier exporté (.csv ou .xlsx)" required>
              <input type="file" accept=".csv,.xlsx,text/csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" onChange={(e) => setFile(e.target.files?.[0] ?? null)} required />
            </Field>
          </div>
          {current && (
            <p className="small">
              Colonnes reconnues :{" "}
              {current.columns.map((c: any) => (
                <span key={c.name} className="tag">
                  {c.name}
                  {c.required ? " *" : ""}
                </span>
              ))}
            </p>
          )}
          <p className="small muted">
            Avant le Personnel : créez les sites en Administration. Dates attendues au format jj/mm/aaaa (ou ISO). Les dossiers, tâches et modèles
            de tâches ne sont pas repris : les parcours en cours se recréent dans l'application.
          </p>
          <ErrorBox error={error} />
          <button className="btn btn-primary" disabled={busy || !file || !list}>
            {busy ? "Analyse…" : "Analyser le fichier (aperçu)"}
          </button>
        </form>
      </Section>
      <Section title="Lots de reprise">
        {batches.data?.length === 0 ? (
          <Empty>Aucun lot.</Empty>
        ) : (
          <table>
            <tbody>
              {batches.data?.map((b) => (
                <tr key={b.id}>
                  <td>
                    <Link to={`/administration/reprise/${b.id}`}>
                      <strong>{b.source_list}</strong>
                    </Link>{" "}
                    <span className="small">{b.file_name}</span>
                    <div className="small muted">
                      {fmtDateTime(b.created_at)} · {b.created_by}
                    </div>
                  </td>
                  <td>
                    <Badge status={b.status === "applique" ? "termine" : b.status === "abandonne" ? "annule" : "en_cours"} label={b.status === "applique" ? "Appliqué" : b.status === "abandonne" ? "Abandonné" : "Aperçu"} />
                  </td>
                  <td className="small">{b.summary?.lignes} ligne(s)</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
    </div>
  );
}

export function RepriseBatchPage() {
  const { id = "" } = useParams();
  const d = useLoad(() => api<any>(`/reprise/batches/${id}`), [id]);
  const [filter, setFilter] = useState("");
  const [confirming, setConfirming] = useState(false);
  const cmd = useCommand();
  if (d.loading && !d.data) return <Loading />;
  if (d.error) return <ErrorBox error={d.error} onRetry={d.reload} />;
  const { batch: b, rows } = d.data;
  const preview = b.status === "apercu";
  const shown = rows.filter((r: any) => !filter || r.proposal === filter || r.result === filter);
  const counts: Record<string, number> = {};
  for (const r of rows) counts[r.proposal] = (counts[r.proposal] ?? 0) + 1;
  const undecided = rows.filter((r: any) => r.proposal === "a_revoir" && !r.decision).length;
  const decide = async (row: number, decision: string, target_id?: string) => {
    const r = await cmd.run(() => api(`/reprise/batches/${id}/rows/${row}/decision`, { body: { decision, target_id: target_id ?? null } }));
    if (r) d.reload();
  };
  return (
    <>
      <PageHeader
        title={`Reprise ${b.source_list}`}
        subtitle={
          <>
            {b.file_name} · {fmtDateTime(b.created_at)} · {b.status === "applique" ? `appliqué le ${fmtDateTime(b.committed_at)}` : b.status === "abandonne" ? "abandonné" : "aperçu (rien n'est encore écrit)"}
          </>
        }
        actions={
          <>
            <Link className="btn" to="/administration?onglet=reprise">
              Retour
            </Link>
            {preview && (
              <>
                <button className="btn" onClick={async () => { await cmd.run(() => api(`/reprise/batches/${id}/abandon`, { body: {} })); d.reload(); }}>
                  Abandonner
                </button>
                <button className="btn btn-primary" onClick={() => setConfirming(true)}>
                  Appliquer le lot
                </button>
              </>
            )}
          </>
        }
      />
      {b.summary?.avec_id === false && <Notice kind="warn">Export sans colonne ID : les liens passent par les noms affichés et un rejeu ne reconnaît pas les lignes déjà reprises.</Notice>}
      {b.summary?.application && (
        <Notice kind="ok">
          Résultat :{" "}
          {Object.entries(b.summary.application.resultats as Record<string, number>)
            .map(([k, n]) => `${repriseResultLabel[k] ?? k} ${n}`)
            .join(" · ")}
          {b.summary.application.managers_relies > 0 && ` · ${b.summary.application.managers_relies} manager(s) relié(s)`}
        </Notice>
      )}
      <ErrorBox error={cmd.error} />
      <div className="filters">
        <label className="field">
          <span className="field-label">Afficher</span>
          <select value={filter} onChange={(e) => setFilter(e.target.value)}>
            <option value="">Toutes les lignes ({rows.length})</option>
            {Object.entries(counts).map(([k, n]) => (
              <option key={k} value={k}>
                {repriseProposalLabel[k]} ({n})
              </option>
            ))}
            {!preview && <option value="conflit">En conflit</option>}
          </select>
        </label>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th className="num">Ligne</th>
              <th>Données lues</th>
              <th>Proposition</th>
              <th>Messages</th>
              <th>{preview ? "Décision" : "Résultat"}</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((r: any) => (
              <tr key={r.row_number}>
                <td className="num">{r.row_number}</td>
                <td className="small" style={{ maxWidth: 320 }}>
                  {Object.entries(r.raw as Record<string, string>)
                    .slice(0, 6)
                    .map(([k, v]) => (
                      <div key={k} dir="auto">
                        <span className="muted">{k} :</span> {v}
                      </div>
                    ))}
                </td>
                <td>
                  <Badge status={r.proposal} label={repriseProposalLabel[r.proposal]} />
                </td>
                <td className="small">
                  {r.messages.map((m: any, i: number) => (
                    <div key={i} style={{ color: m.niveau === "erreur" ? "var(--danger)" : m.niveau === "attention" ? "var(--warn)" : undefined }}>
                      {m.texte}
                    </div>
                  ))}
                </td>
                <td className="small">
                  {preview ? (
                    r.proposal === "a_revoir" ? (
                      <div className="stack" style={{ gap: ".3rem" }}>
                        {r.decision && <Badge status={r.decision === "exclure" ? "annule" : "termine"} label={r.decision === "exclure" ? "Exclue" : r.decision_target ? "Rattachée" : "Appliquée"} />}
                        {r.candidates.map((c: any) => (
                          <button key={c.id} className="btn btn-small" onClick={() => decide(r.row_number, "appliquer", c.id)} disabled={r.decision_target === c.id}>
                            {b.source_list === "Personnel" ? "Rattacher à" : "Choisir"} {c.nom}
                            {c.matricule ? ` (${c.matricule})` : ""}
                          </button>
                        ))}
                        {b.source_list === "Personnel" && (
                          <button className="btn btn-small" onClick={() => decide(r.row_number, "appliquer")}>
                            Nouvelle personne
                          </button>
                        )}
                        <button className="btn btn-small" onClick={() => decide(r.row_number, "exclure")}>
                          Exclure
                        </button>
                      </div>
                    ) : r.proposal === "rejet" ? (
                      <span className="muted">corriger le fichier</span>
                    ) : (
                      <span className="muted">—</span>
                    )
                  ) : (
                    <>
                      {r.result && <Badge status={r.result} label={repriseResultLabel[r.result]} />}
                      {r.result_message && <div>{r.result_message}</div>}
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {confirming && (
        <Modal title="Appliquer le lot" onClose={() => setConfirming(false)}>
          <p>
            {counts.creation ?? 0} création(s), {counts.mise_a_jour ?? 0} mise(s) à jour, {counts.rejet ?? 0} rejet(s).
            {undecided > 0 && <strong> {undecided} ligne(s) à revoir sans décision ne seront pas appliquées.</strong>}
          </p>
          <p className="small muted">Chaque ligne est appliquée séparément : une ligne en conflit n'empêche pas les autres. Ce fichier ne pourra plus être appliqué une seconde fois.</p>
          <div className="form-actions">
            <button className="btn" onClick={() => setConfirming(false)}>
              Annuler
            </button>
            <button
              className="btn btn-primary"
              disabled={cmd.busy}
              onClick={async () => {
                const r = await cmd.run(() => api(`/reprise/batches/${id}/commit`, { body: {} }));
                setConfirming(false);
                if (r) d.reload();
              }}
            >
              Appliquer
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
