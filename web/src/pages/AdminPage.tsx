import { FormEvent, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api, ApiError } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, Notice, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import { deliveryStatusLabel, fmtDateTime, Role, roleLabel, ROLES } from "../labels";

const TABS = [
  { id: "utilisateurs", label: "Utilisateurs et rôles" },
  { id: "sites", label: "Sites et locaux" },
  { id: "avis", label: "Avis et mails" },
];

export default function AdminPage() {
  const { has } = useAuth();
  const [params, setParams] = useSearchParams();
  const tab = params.get("onglet") ?? "utilisateurs";
  if (!has("ADMIN")) return <Notice kind="danger">Réservé à l'administration.</Notice>;
  return (
    <>
      <PageHeader title="Administration" />
      <div className="row" role="tablist" style={{ marginBottom: "1rem" }}>
        {TABS.map((t) => (
          <button key={t.id} role="tab" aria-selected={tab === t.id} className={`btn ${tab === t.id ? "btn-primary" : ""}`} onClick={() => setParams({ onglet: t.id }, { replace: true })}>
            {t.label}
          </button>
        ))}
      </div>
      {tab === "utilisateurs" && <UsersTab />}
      {tab === "sites" && <SitesTab />}
      {tab === "avis" && <NotificationsTab />}
    </>
  );
}

function ActivationLink({ link }: { link: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Notice kind="ok">
      <strong>Lien d'activation à usage unique (72 h)</strong> — transmettez-le à la personne ; il ne sera plus affiché.
      <div className="row" style={{ marginTop: ".4rem" }}>
        <input readOnly value={link} className="mono small" style={{ flex: 1 }} onFocus={(e) => e.target.select()} />
        <button
          className="btn btn-small"
          onClick={async () => {
            await navigator.clipboard?.writeText(link);
            setCopied(true);
          }}
        >
          {copied ? "Copié" : "Copier"}
        </button>
      </div>
    </Notice>
  );
}

function UsersTab() {
  const { user: me } = useAuth();
  const users = useLoad(() => api<any[]>("/admin/users"), []);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<any | null>(null);
  const [link, setLink] = useState<string | null>(null);
  const cmd = useCommand();

  return (
    <Section title="Utilisateurs" actions={<button className="btn btn-primary" onClick={() => setCreating(true)}>+ Nouvel utilisateur</button>}>
      {link && <ActivationLink link={link} />}
      <ErrorBox error={cmd.error} />
      {users.loading && !users.data ? (
        <Loading />
      ) : (
        <table>
          <thead>
            <tr>
              <th>Nom</th>
              <th>Email</th>
              <th>Rôles</th>
              <th>État</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {users.data?.map((u) => (
              <tr key={u.id}>
                <td>{u.display_name}</td>
                <td className="small">{u.email}</td>
                <td>{u.roles.map((r: Role) => roleLabel[r]).join(" + ")}</td>
                <td>
                  {!u.active ? <Badge status="annule" label="Désactivé" /> : u.activated ? <Badge status="actif" label="Actif" /> : <Badge status="a_faire" label="Activation en attente" />}
                </td>
                <td className="nowrap">
                  <button className="btn btn-small" onClick={() => setEditing(u)}>
                    Rôles
                  </button>{" "}
                  <button
                    className="btn btn-small"
                    onClick={async () => {
                      const r = await cmd.run(() => api<{ activation_link: string }>(`/admin/users/${u.id}/activation`, { body: {} }));
                      if (r) {
                        setLink(r.activation_link);
                        users.reload();
                      }
                    }}
                  >
                    Nouveau lien
                  </button>{" "}
                  {u.id !== me?.id && (
                    <button
                      className={`btn btn-small ${u.active ? "btn-danger" : ""}`}
                      onClick={async () => {
                        await cmd.run(() => api(`/admin/users/${u.id}`, { method: "PATCH", body: { active: !u.active } }));
                        users.reload();
                      }}
                    >
                      {u.active ? "Désactiver" : "Réactiver"}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {creating && (
        <UserDialog
          onClose={() => setCreating(false)}
          onSaved={(l) => {
            setCreating(false);
            setLink(l ?? null);
            users.reload();
          }}
        />
      )}
      {editing && (
        <UserDialog
          user={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            users.reload();
          }}
        />
      )}
    </Section>
  );
}

function UserDialog({ user, onClose, onSaved }: { user?: any; onClose: () => void; onSaved: (link?: string) => void }) {
  const [email, setEmail] = useState(user?.email ?? "");
  const [name, setName] = useState(user?.display_name ?? "");
  const [roles, setRoles] = useState<Role[]>(user?.roles ?? []);
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      if (user) {
        await api(`/admin/users/${user.id}`, { method: "PATCH", body: { display_name: name, roles } });
        onSaved();
      } else {
        const r = await api<{ activation_link: string }>("/admin/users", { body: { email, display_name: name, roles } });
        onSaved(r.activation_link);
      }
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={user ? `Rôles — ${user.display_name}` : "Nouvel utilisateur"} onClose={onClose}>
      <form onSubmit={submit}>
        {!user && (
          <Field label="Email professionnel" required>
            <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} required />
          </Field>
        )}
        <Field label="Nom affiché" required>
          <input value={name} onChange={(e) => setName(e.target.value)} required />
        </Field>
        <fieldset>
          <legend>Rôles (cumulables)</legend>
          <div className="inline-choice">
            {ROLES.map((r) => (
              <label key={r}>
                <input type="checkbox" checked={roles.includes(r)} onChange={(e) => setRoles(e.target.checked ? [...roles, r] : roles.filter((x) => x !== r))} />
                {roleLabel[r]}
              </label>
            ))}
          </div>
        </fieldset>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={roles.length === 0}>
            {user ? "Enregistrer" : "Créer et obtenir le lien"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function SitesTab() {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [site, setSite] = useState({ code: "", name: "" });
  const [locations, setLocations] = useState<Record<string, string>>({});
  const cmd = useCommand();
  return (
    <div className="grid-2">
      <Section title="Sites">
        {sites.data?.length === 0 && <Empty>Aucun site : créez-en au moins un pour annoncer des arrivées.</Empty>}
        {sites.data?.map((s) => (
          <div key={s.id} className="card" style={{ marginBottom: ".6rem" }}>
            <strong>{s.name}</strong> <span className="mono small muted">{s.code}</span>
            <div className="small">Locaux : {s.locations.length ? s.locations.map((l: any) => l.name).join(", ") : "aucun"}</div>
            <form
              className="row"
              style={{ marginTop: ".4rem" }}
              onSubmit={async (e) => {
                e.preventDefault();
                const r = await cmd.run(() => api(`/admin/sites/${s.id}/locations`, { body: { name: locations[s.id] } }));
                if (r) {
                  setLocations({ ...locations, [s.id]: "" });
                  sites.reload();
                }
              }}
            >
              <input aria-label={`Nouveau local pour ${s.name}`} placeholder="Nouveau local" value={locations[s.id] ?? ""} onChange={(e) => setLocations({ ...locations, [s.id]: e.target.value })} required />
              <button className="btn btn-small" type="submit">
                Ajouter
              </button>
            </form>
          </div>
        ))}
      </Section>
      <Section title="Nouveau site">
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            const r = await cmd.run(() => api("/admin/sites", { body: site }));
            if (r) {
              setSite({ code: "", name: "" });
              sites.reload();
            }
          }}
        >
          <Field label="Code" required hint="Court et stable, ex. ORAN">
            <input value={site.code} onChange={(e) => setSite({ ...site, code: e.target.value })} required />
          </Field>
          <Field label="Nom" required>
            <input value={site.name} onChange={(e) => setSite({ ...site, name: e.target.value })} required />
          </Field>
          <ErrorBox error={cmd.error} />
          <button className="btn btn-primary" type="submit">
            Créer le site
          </button>
        </form>
      </Section>
    </div>
  );
}

function NotificationsTab() {
  const [status, setStatus] = useState("");
  const list = useLoad(() => api<any[]>(`/admin/notifications${status ? `?status=${status}` : ""}`), [status]);
  return (
    <Section title="Avis émis et remises">
      <Notice kind="info">
        Mode actuel : <strong>journal</strong> — les avis sont consignés ici et ne partent pas par mail. L'envoi Exchange Online sera activé
        au raccordement Microsoft Graph.
      </Notice>
      <div className="filters">
        <label className="field">
          <span className="field-label">Filtrer</span>
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="">Tous</option>
            <option value="erreur">Routage en erreur</option>
            <option value="en_attente">En attente</option>
            <option value="a_verifier">À vérifier</option>
            <option value="echec">Échec</option>
          </select>
        </label>
        <button className="btn" onClick={list.reload}>
          Actualiser
        </button>
      </div>
      {list.data?.length === 0 ? (
        <Empty>Aucun avis.</Empty>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Date</th>
                <th>Avis</th>
                <th>Équipes</th>
                <th>Destinataires</th>
              </tr>
            </thead>
            <tbody>
              {list.data?.map((n) => (
                <tr key={n.outbox_id}>
                  <td className="nowrap small">{fmtDateTime(n.created_at)}</td>
                  <td>
                    {n.subject}
                    {n.outbox_error && <div className="small" style={{ color: "var(--danger)" }}>{n.outbox_error}</div>}
                  </td>
                  <td className="small">{n.target}</td>
                  <td className="small">
                    {n.deliveries.length === 0 && <Badge status={n.outbox_status} label={n.outbox_status === "erreur" ? "Aucun destinataire" : "En file"} />}
                    {n.deliveries.map((d: any) => (
                      <div key={d.recipient}>
                        {d.recipient} <Badge status={d.status} label={deliveryStatusLabel[d.status]} />
                        {d.sent_to !== d.recipient && <span className="muted"> → {d.sent_to}</span>}
                      </div>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Section>
  );
}
