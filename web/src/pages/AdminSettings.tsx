import { FormEvent, useEffect, useState } from "react";
import { api, ApiError } from "../api";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, Notice, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import { fmtDateTime, roleLabel, Role } from "../labels";

const sourceLabel = (overridden: string[], section: string) =>
  overridden.includes(section) ? <span className="tag">réglé dans l'interface</span> : <span className="tag">valeur du fichier .env</span>;

// ------------------------------------------------------------------ Paramètres

/** Modules, envoi des avis (Exchange) et échéances : réglés ici par l'ADMIN. */
export function ParametresTab() {
  const v = useLoad(() => api<any>("/admin/config"), []);
  if (v.error) return <ErrorBox error={v.error} onRetry={v.reload} />;
  if (!v.data) return <Loading />;
  const d = v.data;
  return (
    <div className="stack">
      {d.problem && (
        <Notice kind="danger">
          Réglages enregistrés mais inapplicables : <strong>{d.problem}</strong>. Les valeurs du fichier .env restent en vigueur.
        </Notice>
      )}
      <ModulesSection data={d} onSaved={v.reload} />
      <MailSection data={d} onSaved={v.reload} />
      <AlertsSection data={d} onSaved={v.reload} />
      <Section title="Version">
        <dl className="kv">
          <dt>Application</dt>
          <dd>{d.app_version}</dd>
          <dt>Adresse publique</dt>
          <dd className="mono">{d.public_url}</dd>
          <dt>Dernière modification</dt>
          <dd>{d.updated_by ? `${fmtDateTime(d.updated_at)} par ${d.updated_by}` : "aucune (valeurs du fichier .env)"}</dd>
        </dl>
      </Section>
    </div>
  );
}

function ResetButton({ section, data, onSaved }: { section: string; data: any; onSaved: () => void }) {
  const cmd = useCommand();
  if (!data.overridden.includes(section)) return null;
  return (
    <>
      <button
        type="button"
        className="btn btn-small"
        disabled={cmd.busy}
        onClick={async () => {
          if (!window.confirm("Revenir aux valeurs du fichier .env pour cette section ?")) return;
          if (await cmd.run(() => api(`/admin/config/${section}`, { method: "DELETE" }))) onSaved();
        }}
      >
        Revenir au fichier .env
      </button>
      <ErrorBox error={cmd.error} />
    </>
  );
}

function ModulesSection({ data, onSaved }: { data: any; onSaved: () => void }) {
  const [off, setOff] = useState<string[]>(data.effective.modules_desactives);
  const cmd = useCommand();
  const [saved, setSaved] = useState(false);
  useEffect(() => setOff(data.effective.modules_desactives), [data]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setSaved(false);
    if (await cmd.run(() => api("/admin/config/modules", { method: "PUT", body: { desactives: off } }))) {
      setSaved(true);
      onSaved();
    }
  };
  return (
    <Section title="Modules ouverts" actions={<ResetButton section="modules" data={data} onSaved={onSaved} />}>
      <p className="small">
        {sourceLabel(data.overridden, "modules")} Le cœur (personnel, arrivées, Mon travail, formations, demandes, parc IT, remises, restitutions,
        décharges, Excel, import) est toujours ouvert. Un module fermé disparaît des menus, ses liens et son API sont refusés et ses traitements
        automatiques s'arrêtent. Les utilisateurs voient le changement en rechargeant la page.
      </p>
      <form onSubmit={submit}>
        <div className="form-grid">
          {data.modules.map((m: any) => (
            <label key={m.name} className="row" style={{ gap: ".4rem" }}>
              <input
                type="checkbox"
                checked={!off.includes(m.name)}
                onChange={(e) => setOff(e.target.checked ? off.filter((x) => x !== m.name) : [...off, m.name])}
              />
              {m.label} {off.includes(m.name) ? <Badge status="muted" label="fermé" /> : <Badge status="ok" label="ouvert" />}
            </label>
          ))}
        </div>
        <ErrorBox error={cmd.error} />
        {saved && <Notice kind="ok">Modules enregistrés.</Notice>}
        <button className="btn btn-primary" disabled={cmd.busy}>
          Enregistrer les modules
        </button>
      </form>
    </Section>
  );
}

function MailSection({ data, onSaved }: { data: any; onSaved: () => void }) {
  const saved = data.saved?.mail ?? {};
  const eff = data.effective;
  const init = () => ({
    mode: saved.mode ?? eff.mail_mode,
    sender: saved.sender ?? eff.mail_sender ?? "",
    test_recipient: saved.test_recipient ?? eff.mail_test_recipient ?? "",
    allowed_domains: (saved.allowed_domains ?? eff.mail_allowed_domains ?? []).join(", "),
    tenant_id: saved.tenant_id ?? eff.mail_tenant_id ?? "",
    client_id: saved.client_id ?? eff.mail_client_id ?? "",
  });
  const [f, setF] = useState(init);
  useEffect(() => setF(init()), [data]); // eslint-disable-line react-hooks/exhaustive-deps
  const cmd = useCommand();
  const certCmd = useCommand();
  const [ok, setOk] = useState<string | null>(null);
  const [testTo, setTestTo] = useState("");
  const [test, setTest] = useState<{ status: string; detail: string } | null>(null);
  const testCmd = useCommand();
  const cert = data.certificates.envoi;
  const body = (mode: string) => ({
    ...f,
    mode,
    allowed_domains: f.allowed_domains.split(/[,;\s]+/).filter(Boolean),
  });
  const save = async (mode: string) => {
    setOk(null);
    if (mode === "graph" && !window.confirm("Activer l'envoi réel des avis par Exchange ? Les avis créés avant l'activation ne partiront pas.")) return;
    if (await cmd.run(() => api("/admin/config/mail", { method: "PUT", body: body(mode) }))) {
      setOk(mode === "graph" ? "Envoi par Exchange activé." : "Réglages enregistrés : aucun mail n'est envoyé (mode journal).");
      onSaved();
    }
  };
  const generate = async () => {
    const replace = cert.source === "interface";
    if (replace && !window.confirm("Remplacer le certificat ? L'ancien devra être retiré de l'application Entra, et le nouveau y être déposé.")) return;
    if (await certCmd.run(() => api("/admin/config/certificates/envoi", { body: { replace } }))) onSaved();
  };
  return (
    <Section title="Avis par mail (Exchange Online)" actions={<ResetButton section="mail" data={data} onSaved={onSaved} />}>
      <p className="small">
        {sourceLabel(data.overridden, "mail")} État actuel :{" "}
        {eff.mail_mode === "graph" ? (
          <Badge status="ok" label={`envoi par Exchange depuis ${eff.mail_sender}`} />
        ) : (
          <Badge status="muted" label="mode journal : aucun mail envoyé, les équipes consultent « Mon travail »" />
        )}
      </p>
      <details>
        <summary>Marche à suivre avec l'administrateur Microsoft 365</summary>
        <ol className="small">
          <li>Créer une boîte d'expédition dédiée (ex. avis-parcours@…).</li>
          <li>
            Ici : <strong>Générer le certificat</strong> puis le <strong>télécharger</strong>. La clé privée reste sur le serveur.
          </li>
          <li>Dans Entra : créer l'application « Celluloplast — envoi », y déposer le certificat (Certificats et secrets → Certificats).</li>
          <li>Donner le droit « Mail.Send » en application, limité à cette seule boîte (Exchange « Application RBAC »).</li>
          <li>Ici : saisir le locataire, l'application et la boîte, enregistrer, puis envoyer un mail de test.</li>
          <li>Quand le test est reçu : « Activer l'envoi ».</li>
        </ol>
      </details>
      <div className="card" style={{ margin: ".6rem 0" }}>
        <strong>Certificat d'envoi</strong>{" "}
        {cert.source === "aucun" ? (
          <Badge status="warn" label="aucun" />
        ) : (
          <Badge status="ok" label={cert.source === "interface" ? "généré sur le serveur" : "fourni par le fichier .env"} />
        )}
        {cert.details && (
          <div className="small mono">
            {cert.details.subject} · empreinte {cert.details.thumbprint_sha1} · valable jusqu'au {fmtDateTime(cert.details.not_after)}
          </div>
        )}
        <div className="row" style={{ marginTop: ".4rem" }}>
          <button type="button" className="btn btn-small" onClick={generate} disabled={certCmd.busy}>
            {cert.source === "interface" ? "Remplacer le certificat" : "Générer le certificat"}
          </button>
          {cert.source !== "aucun" && (
            <a className="btn btn-small" href="/api/v1/admin/config/certificates/envoi" download>
              Télécharger le certificat (.crt)
            </a>
          )}
        </div>
        <ErrorBox error={certCmd.error} />
      </div>
      <div className="form-grid">
        <Field label="Locataire (Directory / tenant ID)">
          <input value={f.tenant_id} onChange={(e) => setF({ ...f, tenant_id: e.target.value })} className="mono" />
        </Field>
        <Field label="Application (client ID)">
          <input value={f.client_id} onChange={(e) => setF({ ...f, client_id: e.target.value })} className="mono" />
        </Field>
        <Field label="Boîte d'expédition">
          <input type="email" value={f.sender} onChange={(e) => setF({ ...f, sender: e.target.value })} placeholder="avis-parcours@…" />
        </Field>
        <Field label="Domaines autorisés" hint="Pendant le pilote : seuls ces domaines reçoivent des mails (ex. votre-entreprise.com). Vide : tous.">
          <input value={f.allowed_domains} onChange={(e) => setF({ ...f, allowed_domains: e.target.value })} />
        </Field>
        <Field label="Redirection de test" hint="Facultatif : tous les avis partent vers cette seule adresse.">
          <input type="email" value={f.test_recipient} onChange={(e) => setF({ ...f, test_recipient: e.target.value })} />
        </Field>
      </div>
      <ErrorBox error={cmd.error} />
      {ok && <Notice kind="ok">{ok}</Notice>}
      <div className="row">
        <button type="button" className="btn" disabled={cmd.busy} onClick={() => save("journal")}>
          Enregistrer sans envoyer (mode journal)
        </button>
        <button type="button" className="btn btn-primary" disabled={cmd.busy} onClick={() => save("graph")}>
          Activer l'envoi par Exchange
        </button>
      </div>
      <form
        className="row"
        style={{ marginTop: ".8rem" }}
        onSubmit={async (e) => {
          e.preventDefault();
          setTest(null);
          const r = await testCmd.run(() => api<{ status: string; detail: string }>("/admin/config/mail-test", { body: { to: testTo } }));
          if (r) setTest(r);
        }}
      >
        <input type="email" required placeholder="Adresse de test" value={testTo} onChange={(e) => setTestTo(e.target.value)} aria-label="Adresse du mail de test" />
        <button className="btn" disabled={testCmd.busy}>
          {testCmd.busy ? "Envoi…" : "Envoyer un mail de test"}
        </button>
      </form>
      <p className="small muted">Le test utilise les réglages enregistrés, même en mode journal. « Accepté » n'est pas une preuve de réception : vérifiez la boîte.</p>
      <ErrorBox error={testCmd.error} />
      {test && <Notice kind={test.status === "accepte" ? "ok" : test.status === "inconnu" ? "warn" : "danger"}>{test.detail}</Notice>}
    </Section>
  );
}

function AlertsSection({ data, onSaved }: { data: any; onSaved: () => void }) {
  const e = data.effective;
  const [f, setF] = useState({ alert_days: e.alert_days, alert_km: e.alert_km, escalation_minutes: e.escalation_minutes });
  useEffect(() => setF({ alert_days: data.effective.alert_days, alert_km: data.effective.alert_km, escalation_minutes: data.effective.escalation_minutes }), [data]);
  const cmd = useCommand();
  const [ok, setOk] = useState(false);
  return (
    <Section title="Échéances et relances" actions={<ResetButton section="alerts" data={data} onSaved={onSaved} />}>
      <p className="small">{sourceLabel(data.overridden, "alerts")}</p>
      <form
        onSubmit={async (ev) => {
          ev.preventDefault();
          setOk(false);
          if (await cmd.run(() => api("/admin/config/alerts", { method: "PUT", body: f }))) {
            setOk(true);
            onSaved();
          }
        }}
      >
        <div className="form-grid">
          <Field label="Alerte avant échéance (jours)" hint="Pièces de véhicule, location, abonnements logiciels.">
            <input type="number" min={1} max={365} value={f.alert_days} onChange={(ev) => setF({ ...f, alert_days: Number(ev.target.value) })} />
          </Field>
          <Field label="Alerte avant entretien (km)">
            <input type="number" min={0} max={100000} value={f.alert_km} onChange={(ev) => setF({ ...f, alert_km: Number(ev.target.value) })} />
          </Field>
          <Field label="Relance « accès à vérifier » (minutes)" hint="Après l'heure de coupure d'un départ.">
            <input type="number" min={5} max={1440} value={f.escalation_minutes} onChange={(ev) => setF({ ...f, escalation_minutes: Number(ev.target.value) })} />
          </Field>
        </div>
        <ErrorBox error={cmd.error} />
        {ok && <Notice kind="ok">Échéances enregistrées.</Notice>}
        <button className="btn btn-primary" disabled={cmd.busy}>
          Enregistrer
        </button>
      </form>
    </Section>
  );
}

// ------------------------------------------------------------------ Sauvegardes

const runStatus: Record<string, string> = { demande: "demandée", en_cours: "en cours", reussie: "réussie", echec: "échec" };
const runTone: Record<string, string> = { demande: "info", en_cours: "info", reussie: "ok", echec: "danger" };

function size(n: number) {
  if (n > 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} Mo`;
  if (n > 1024) return `${(n / 1024).toFixed(0)} Ko`;
  return `${n} o`;
}

export function SauvegardesTab() {
  const v = useLoad(() => api<any>("/admin/config"), []);
  const runs = useLoad(() => api<any[]>("/admin/backups"), []);
  const [key, setKey] = useState("");
  const [hour, setHour] = useState(2);
  const [keep, setKeep] = useState(14);
  const cmd = useCommand();
  const runCmd = useCommand();
  const [ok, setOk] = useState<string | null>(null);
  useEffect(() => {
    if (v.data) {
      setHour(v.data.backup.hour);
      setKeep(v.data.backup.keep);
    }
  }, [v.data]);
  // Rafraîchit la liste tant qu'une sauvegarde est en cours.
  const active = runs.data?.some((r) => r.status === "demande" || r.status === "en_cours");
  useEffect(() => {
    if (!active) {
      // Fin d'une sauvegarde : « dernière réussite » et alertes à jour.
      v.reload();
      return;
    }
    const t = window.setInterval(() => runs.reload(), 4000);
    return () => window.clearInterval(t);
  }, [active]); // eslint-disable-line react-hooks/exhaustive-deps
  if (v.error) return <ErrorBox error={v.error} onRetry={v.reload} />;
  if (!v.data) return <Loading />;
  const b = v.data.backup;
  const stale = b.configured && (!b.last_success_at || Date.now() - new Date(b.last_success_at).getTime() > 26 * 3600 * 1000);
  return (
    <div className="stack">
      {!b.configured && (
        <Notice kind="danger">
          <strong>Aucune sauvegarde n'est configurée.</strong> Avant de saisir des données réelles, collez ci-dessous la clé publique de récupération.
        </Notice>
      )}
      {stale && <Notice kind="warn">Aucune sauvegarde réussie depuis plus de 26 heures : vérifiez la dernière tentative ci-dessous.</Notice>}
      {(!b.gpg_ok || !b.pg_dump_ok) && (
        <Notice kind="danger">
          Outils manquants sur le serveur : {!b.gpg_ok && "gpg "} {!b.pg_dump_ok && "pg_dump"} (image Docker antérieure : reconstruisez-la).
        </Notice>
      )}
      <Section title="Clé de récupération et planification" actions={<ResetButton section="backup" data={v.data} onSaved={v.reload} />}>
        <p className="small">
          Les sauvegardes sont <strong>chiffrées</strong> avec une clé publique GnuPG. La <strong>clé privée</strong> correspondante reste hors du
          serveur (coffre de la DSI) : sans elle, personne ne peut lire les sauvegardes, pas même depuis le serveur. Elle est indispensable pour
          restaurer — ne la perdez pas.
        </p>
        <details>
          <summary>Créer la paire de clés (une fois, sur un poste d'administration)</summary>
          <p className="small">Avec Gpg4win (Kleopatra) : Fichier → Nouvelle paire de clés → « Créer une paire de clés OpenPGP personnelle », nom « Celluloplast sauvegarde », phrase de passe forte. Puis clic droit → Exporter… pour obtenir la clé <strong>publique</strong> (.asc) à coller ici, et « Sauvegarder les clés secrètes » vers une clé USB rangée au coffre. En ligne de commande :</p>
          <pre className="small mono">{`gpg --quick-generate-key "Celluloplast sauvegarde" rsa4096 encr 5y
gpg --armor --export "Celluloplast sauvegarde" > cle-publique.asc
gpg --armor --export-secret-keys "Celluloplast sauvegarde" > cle-privee.asc   # au coffre, jamais sur le serveur`}</pre>
        </details>
        {b.configured && (
          <dl className="kv" style={{ margin: ".6rem 0" }}>
            <dt>Clé en place</dt>
            <dd>
              {b.uid} <span className="mono small">{b.fingerprint}</span>
            </dd>
            <dt>Planification</dt>
            <dd>
              chaque jour à {b.hour} h (heure d'Alger), {b.keep} sauvegardes gardées sur le serveur
            </dd>
            <dt>Dernière réussite</dt>
            <dd>{b.last_success_at ? fmtDateTime(b.last_success_at) : "aucune"}</dd>
          </dl>
        )}
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            setOk(null);
            const body: any = { hour, keep };
            if (key.trim()) body.public_key = key;
            if (await cmd.run(() => api("/admin/config/backup", { method: "PUT", body }))) {
              setKey("");
              setOk("Sauvegarde configurée.");
              v.reload();
            }
          }}
        >
          <Field label={b.configured ? "Remplacer la clé publique (facultatif)" : "Clé publique de récupération"} required={!b.configured}>
            <textarea rows={5} className="mono small" value={key} onChange={(e) => setKey(e.target.value)} placeholder="-----BEGIN PGP PUBLIC KEY BLOCK-----" />
          </Field>
          <div className="form-grid">
            <Field label="Heure de la sauvegarde quotidienne">
              <select value={hour} onChange={(e) => setHour(Number(e.target.value))}>
                {Array.from({ length: 24 }, (_, h) => (
                  <option key={h} value={h}>
                    {h} h
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Sauvegardes gardées sur le serveur" hint="Les plus anciennes sont effacées du serveur (pas de vos copies).">
              <input type="number" min={1} max={90} value={keep} onChange={(e) => setKeep(Number(e.target.value))} />
            </Field>
          </div>
          <ErrorBox error={cmd.error} />
          {ok && <Notice kind="ok">{ok}</Notice>}
          <button className="btn btn-primary" disabled={cmd.busy}>
            Enregistrer
          </button>
        </form>
      </Section>
      <Section
        title="Sauvegardes"
        actions={
          <button
            className="btn btn-primary"
            disabled={!b.configured || runCmd.busy || active}
            onClick={async () => {
              if (await runCmd.run(() => api("/admin/backups", { body: {} }))) runs.reload();
            }}
          >
            Sauvegarder maintenant
          </button>
        }
      >
        <ErrorBox error={runCmd.error} />
        <Notice kind="info">
          Un disque unique n'est pas une sauvegarde : téléchargez régulièrement les fichiers d'une sauvegarde réussie (les quatre) et rangez-les hors
          de la VM. Ils sont chiffrés : seul le détenteur de la clé privée peut les lire.
        </Notice>
        {!b.caddy_mounted && (
          <p className="small muted">
            Le certificat HTTPS du serveur n'est pas dans ces sauvegardes : après une restauration sur une autre machine, les PC devront réinstaller
            le certificat racine (ou sauvegardez-le une fois avec deploy/backup.sh).
          </p>
        )}
        {runs.data?.length === 0 && <Empty>Aucune sauvegarde pour l'instant.</Empty>}
        {runs.data && runs.data.length > 0 && (
          <table>
            <thead>
              <tr>
                <th>Date</th>
                <th>État</th>
                <th>Fichiers</th>
              </tr>
            </thead>
            <tbody>
              {runs.data.map((r) => (
                <tr key={r.id}>
                  <td>
                    {fmtDateTime(r.finished_at ?? r.started_at ?? r.requested_at)}
                    <div className="small muted">{r.trigger === "auto" ? "quotidienne" : `manuelle${r.requested_by ? ` (${r.requested_by})` : ""}`}</div>
                  </td>
                  <td>
                    <Badge status={runTone[r.status]} label={runStatus[r.status] ?? r.status} />
                    {r.error && <div className="small" style={{ color: "var(--danger)" }}>{r.error}</div>}
                    {(r.warnings ?? []).map((w: string) => (
                      <div key={w} className="small muted">
                        {w}
                      </div>
                    ))}
                  </td>
                  <td>
                    {r.purged_at ? (
                      <span className="small muted">effacée du serveur (rotation) le {fmtDateTime(r.purged_at)}</span>
                    ) : (
                      (r.files ?? []).map((f: any) => (
                        <div key={f.name} className="small">
                          <a href={`/api/v1/admin/backups/${r.id}/files/${encodeURIComponent(f.name)}`} download>
                            {f.name}
                          </a>{" "}
                          <span className="muted">({size(f.size)})</span>
                        </div>
                      ))
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
      <Section title="Restaurer une sauvegarde">
        <p className="small">
          Sur un poste d'administration qui détient la clé privée : vérifier les empreintes (<span className="mono">sha256sum -c sha256-….txt</span>), puis
          déchiffrer (<span className="mono">gpg -d cellulo-….dump.gpg &gt; cellulo.dump</span>, idem pour les documents et les clés). La procédure complète
          (machine de test, script <span className="mono">deploy/restore-isole.sh</span>) est décrite dans <span className="mono">deploy/README.md</span>.
          L'exercice de restauration complet n'a pas encore été exécuté : faites-le une fois avant de vous reposer sur ces sauvegardes.
        </p>
      </Section>
    </div>
  );
}

// ------------------------------------------------------------------ Modèles de tâches

export function ModelesTab() {
  const v = useLoad(() => api<any[]>("/task-templates"), []);
  const [editing, setEditing] = useState<any | null>(null);
  if (v.error) return <ErrorBox error={v.error} onRetry={v.reload} />;
  if (!v.data) return <Loading />;
  return (
    <Section title="Modèles de tâches">
      <p className="small">
        Pour chaque type de tâche créée à l'annonce d'une arrivée ou d'une demande : libellé, consignes affichées à l'équipe, échéance (en jours par
        rapport à la date d'arrivée ou à la date souhaitée : négatif = avant) et ordre d'affichage. Un modèle s'applique aux tâches créées{" "}
        <strong>après</strong> sa modification ; les tâches existantes ne sont pas réécrites.
      </p>
      <table>
        <thead>
          <tr>
            <th>Type</th>
            <th>Équipe</th>
            <th>Libellé</th>
            <th>Échéance</th>
            <th>Ordre</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {[...v.data]
            .sort((a, b) => a.display_order - b.display_order || a.default_title.localeCompare(b.default_title))
            .map((t) => (
              <tr key={t.task_key}>
                <td>
                  {t.default_title}
                  {t.instructions && <div className="small muted">consignes : {t.instructions.slice(0, 80)}{t.instructions.length > 80 ? "…" : ""}</div>}
                </td>
                <td>{roleLabel[t.role as Role] ?? t.role}</td>
                <td>{t.title ?? <span className="muted">par défaut</span>}</td>
                <td>{t.delay_days == null ? <span className="muted">date d'arrivée</span> : `${t.delay_days > 0 ? "+" : ""}${t.delay_days} j`}</td>
                <td>{t.display_order}</td>
                <td>
                  <button className="btn btn-small" onClick={() => setEditing(t)}>
                    Modifier
                  </button>
                </td>
              </tr>
            ))}
        </tbody>
      </table>
      {editing && (
        <TemplateDialog
          template={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            v.reload();
          }}
        />
      )}
    </Section>
  );
}

function TemplateDialog({ template, onClose, onSaved }: { template: any; onClose: () => void; onSaved: () => void }) {
  const [f, setF] = useState({
    title: template.title ?? "",
    instructions: template.instructions ?? "",
    delay_days: template.delay_days == null ? "" : String(template.delay_days),
    display_order: template.display_order,
  });
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api(`/admin/task-templates/${encodeURIComponent(template.task_key)}`, {
        method: "PUT",
        body: {
          title: f.title,
          instructions: f.instructions,
          delay_days: f.delay_days === "" ? null : Number(f.delay_days),
          display_order: Number(f.display_order),
        },
      });
      onSaved();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={`Modèle : ${template.default_title}`} onClose={onClose} wide>
      <form onSubmit={submit}>
        <Field label="Libellé de la tâche" hint={`Vide : « ${template.default_title} »`}>
          <input value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} maxLength={120} />
        </Field>
        <Field label="Consignes pour l'équipe" hint="Affichées dans « Mon travail » et dans le dossier.">
          <textarea rows={5} value={f.instructions} onChange={(e) => setF({ ...f, instructions: e.target.value })} maxLength={4000} />
        </Field>
        <div className="form-grid">
          <Field label="Échéance (jours)" hint="Par rapport à la date d'arrivée : -3 = trois jours avant. Vide : la date d'arrivée.">
            <input type="number" min={-365} max={365} value={f.delay_days} onChange={(e) => setF({ ...f, delay_days: e.target.value })} />
          </Field>
          <Field label="Ordre d'affichage" hint="Les plus petits d'abord.">
            <input type="number" min={0} max={9999} value={f.display_order} onChange={(e) => setF({ ...f, display_order: Number(e.target.value) })} />
          </Field>
        </div>
        <ErrorBox error={error} />
        <div className="row">
          <button className="btn btn-primary">Enregistrer</button>
          {template.customized && (
            <button
              type="button"
              className="btn"
              onClick={async () => {
                try {
                  await api(`/admin/task-templates/${encodeURIComponent(template.task_key)}`, { method: "DELETE" });
                  onSaved();
                } catch (err) {
                  setError(err as ApiError);
                }
              }}
            >
              Revenir au modèle par défaut
            </button>
          )}
        </div>
      </form>
    </Modal>
  );
}
