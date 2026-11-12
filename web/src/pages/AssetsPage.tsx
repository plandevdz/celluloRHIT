import { FormEvent, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { api, ApiError } from "../api";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import { assetKindLabel, assetStatusLabel } from "../labels";

const TABS = [
  { id: "actifs", label: "Actifs" },
  { id: "accessoires", label: "Accessoires" },
  { id: "sim", label: "SIM et lignes" },
];

export default function AssetsPage() {
  const [params, setParams] = useSearchParams();
  const tab = params.get("onglet") ?? "actifs";
  return (
    <>
      <PageHeader title="Parc IT" subtitle="Disponibles, remis, en contrôle ; accessoires en stock ; SIM et lignes." />
      <div className="row" role="tablist" style={{ marginBottom: "1rem" }}>
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={tab === t.id}
            className={`btn ${tab === t.id ? "btn-primary" : ""}`}
            onClick={() => setParams({ onglet: t.id }, { replace: true })}
          >
            {t.label}
          </button>
        ))}
      </div>
      {tab === "actifs" && <AssetsTab />}
      {tab === "accessoires" && <AccessoriesTab />}
      {tab === "sim" && <SimsTab />}
    </>
  );
}

function AssetsTab() {
  const navigate = useNavigate();
  const [filters, setFilters] = useState({ q: "", status: "", kind: "" });
  const [applied, setApplied] = useState(filters);
  const applyNow = (f: typeof filters) => {
    setFilters(f);
    setApplied(f);
  };
  const q = new URLSearchParams(Object.entries(applied).filter(([, v]) => v));
  const list = useLoad(() => api<any[]>(`/assets?${q}`), [q.toString()]);
  const [scan, setScan] = useState("");
  const [scanError, setScanError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const onScan = async (e: FormEvent) => {
    e.preventDefault();
    setScanError(null);
    const code = scan.trim();
    if (!code) return;
    try {
      const a = await api<any>(`/assets/lookup/${encodeURIComponent(code)}`);
      navigate(`/parc/actifs/${a.id}`);
    } catch (err) {
      setScanError((err as ApiError).message);
      setScan("");
    }
  };

  return (
    <div className="stack">
      <Section title="Scanner un actif" actions={<button className="btn btn-primary" onClick={() => setCreating(true)}>+ Entrée en stock</button>}>
        <form onSubmit={onScan}>
          <input className="scan-input" value={scan} onChange={(e) => setScan(e.target.value)} placeholder="QR ou n° d'inventaire puis Entrée" aria-label="Code à scanner" />
        </form>
        {scanError && <div className="alert alert-danger">{scanError}</div>}
      </Section>
      <Section title={`Actifs (${list.data?.length ?? 0})`}>
        <form
          className="filters"
          onSubmit={(e) => {
            e.preventDefault();
            setApplied(filters);
          }}
        >
          <label className="field">
            <span className="field-label">Recherche</span>
            <input value={filters.q} onChange={(e) => setFilters({ ...filters, q: e.target.value })} placeholder="Inventaire, série, libellé" />
          </label>
          <label className="field">
            <span className="field-label">État</span>
            <select value={filters.status} onChange={(e) => applyNow({ ...filters, status: e.target.value })}>
              <option value="">Tous</option>
              {Object.entries(assetStatusLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span className="field-label">Type</span>
            <select value={filters.kind} onChange={(e) => applyNow({ ...filters, kind: e.target.value })}>
              <option value="">Tous</option>
              {Object.entries(assetKindLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </label>
          <button className="btn" type="submit">
            Filtrer
          </button>
        </form>
        <ErrorBox error={list.error} onRetry={list.reload} />
        {list.loading && !list.data ? (
          <Loading />
        ) : list.data?.length === 0 ? (
          <Empty>Aucun actif.</Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Inventaire</th>
                  <th>Libellé</th>
                  <th>Type</th>
                  <th>Série</th>
                  <th>État</th>
                  <th>Détenteur</th>
                  <th>Site · local</th>
                </tr>
              </thead>
              <tbody>
                {list.data?.map((a) => (
                  <tr key={a.id}>
                    <td className="mono">
                      <Link to={`/parc/actifs/${a.id}`}>{a.inventory_no}</Link>
                    </td>
                    <td>{a.label}</td>
                    <td>{assetKindLabel[a.kind]}</td>
                    <td className="mono small">{a.serial_no ?? "—"}</td>
                    <td>
                      <Badge status={a.status} label={assetStatusLabel[a.status]} />
                    </td>
                    <td>{a.holder_id ? <Link to={`/personnel/${a.holder_id}`}>{a.holder_name}</Link> : "—"}</td>
                    <td className="small">
                      {a.site_name ?? "—"}
                      {a.location_name && ` · ${a.location_name}`}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Section>
      {creating && <CreateAssetDialog onClose={() => setCreating(false)} onCreated={(id) => navigate(`/parc/actifs/${id}`)} />}
    </div>
  );
}

function CreateAssetDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({ inventory_no: "", kind: "pc_portable", label: "", serial_no: "", site_id: "", location_id: "", notes: "" });
  const [error, setError] = useState<ApiError | null>(null);
  const site = sites.data?.find((s) => s.id === f.site_id);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      const r = await api<{ id: string }>("/assets", {
        body: { ...f, serial_no: f.serial_no || null, site_id: f.site_id || null, location_id: f.location_id || null, notes: f.notes || null },
      });
      onCreated(r.id);
    } catch (err) {
      setError(err as ApiError);
    }
  };
  const bind = (k: keyof typeof f) => ({ value: f[k], onChange: (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value }) });
  return (
    <Modal title="Entrée en stock d'un actif" onClose={onClose}>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="N° d'inventaire" required>
            <input {...bind("inventory_no")} required placeholder="PC-045" />
          </Field>
          <Field label="Type" required>
            <select {...bind("kind")}>
              {Object.entries(assetKindLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <Field label="Libellé" required>
          <input {...bind("label")} required placeholder="Portable Dell Latitude 5440" />
        </Field>
        <Field label="N° de série">
          <input {...bind("serial_no")} />
        </Field>
        <div className="form-grid">
          <Field label="Site">
            <select {...bind("site_id")}>
              <option value="">—</option>
              {sites.data?.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Local">
            <select {...bind("location_id")} disabled={!site}>
              <option value="">—</option>
              {site?.locations.map((l: any) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <p className="small muted">Un jeton QR permanent est attribué automatiquement (sans donnée personnelle).</p>
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

function AccessoriesTab() {
  const list = useLoad(() => api<any[]>("/accessories"), []);
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({ label: "", site_id: "", quantity: 0 });
  const [adjust, setAdjust] = useState<{ id: string; label: string; delta: number } | null>(null);
  const cmd = useCommand();
  const create = async (e: FormEvent) => {
    e.preventDefault();
    const r = await cmd.run(() => api("/accessories", { body: { ...f, site_id: f.site_id || null } }));
    if (r) {
      setF({ label: "", site_id: "", quantity: 0 });
      list.reload();
    }
  };
  return (
    <div className="grid-2">
      <Section title="Accessoires (stock)">
        {list.data?.length === 0 ? (
          <Empty>Aucun accessoire.</Empty>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Accessoire</th>
                <th>Site</th>
                <th className="num">En stock</th>
                <th className="num">Remis</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {list.data?.map((a) => (
                <tr key={a.id}>
                  <td>{a.label}</td>
                  <td>{a.site_name ?? "—"}</td>
                  <td className="num">{a.quantity_on_hand}</td>
                  <td className="num">{a.quantity_out}</td>
                  <td className="nowrap">
                    <button className="btn btn-small" onClick={() => setAdjust({ id: a.id, label: a.label, delta: 1 })}>
                      Ajuster
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
      <Section title="Nouvel accessoire">
        <form onSubmit={create}>
          <Field label="Libellé" required>
            <input value={f.label} onChange={(e) => setF({ ...f, label: e.target.value })} required placeholder="Chargeur PC portable" />
          </Field>
          <div className="form-grid">
            <Field label="Site">
              <select value={f.site_id} onChange={(e) => setF({ ...f, site_id: e.target.value })}>
                <option value="">—</option>
                {sites.data?.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Quantité initiale">
              <input type="number" min={0} value={f.quantity} onChange={(e) => setF({ ...f, quantity: Number(e.target.value) })} />
            </Field>
          </div>
          <ErrorBox error={cmd.error} />
          <div className="form-actions">
            <button className="btn btn-primary" type="submit">
              Créer
            </button>
          </div>
        </form>
      </Section>
      {adjust && (
        <AdjustDialog
          item={adjust}
          onClose={() => setAdjust(null)}
          onDone={() => {
            setAdjust(null);
            list.reload();
          }}
        />
      )}
    </div>
  );
}

function AdjustDialog({ item, onClose, onDone }: { item: { id: string; label: string; delta: number }; onClose: () => void; onDone: () => void }) {
  const [delta, setDelta] = useState(item.delta);
  const [reason, setReason] = useState("");
  const cmd = useCommand();
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const r = await cmd.run(() => api(`/accessories/${item.id}/stock`, { body: { delta, reason } }));
    if (r) onDone();
  };
  return (
    <Modal title={`Ajuster le stock — ${item.label}`} onClose={onClose}>
      <form onSubmit={submit}>
        <Field label="Variation (+ entrée, − sortie)" required>
          <input type="number" value={delta} onChange={(e) => setDelta(Number(e.target.value))} required />
        </Field>
        <Field label="Motif (entrée en stock, casse, inventaire…)" required>
          <input value={reason} onChange={(e) => setReason(e.target.value)} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy || delta === 0 || !reason.trim()}>
            Enregistrer
          </button>
        </div>
      </form>
    </Modal>
  );
}

function SimsTab() {
  const list = useLoad(() => api<any[]>("/sims"), []);
  const [f, setF] = useState({ iccid: "", line_number: "", operator: "" });
  const cmd = useCommand();
  const create = async (e: FormEvent) => {
    e.preventDefault();
    const r = await cmd.run(() => api("/sims", { body: { iccid: f.iccid, line_number: f.line_number || null, operator: f.operator || null } }));
    if (r) {
      setF({ iccid: "", line_number: "", operator: "" });
      list.reload();
    }
  };
  return (
    <div className="grid-2">
      <Section title="Cartes SIM">
        {list.data?.length === 0 ? (
          <Empty>Aucune SIM.</Empty>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Ligne</th>
                <th>Référence SIM</th>
                <th>État</th>
                <th>Détenteur</th>
              </tr>
            </thead>
            <tbody>
              {list.data?.map((s) => (
                <tr key={s.id}>
                  <td>
                    {s.line_number ?? "—"}
                    {s.operator && <div className="small muted">{s.operator}</div>}
                  </td>
                  <td className="mono small">{s.iccid}</td>
                  <td>
                    <Badge status={s.status === "affecte" ? "affecte" : s.status} label={s.status === "affecte" ? "Remise" : s.status === "disponible" ? "Disponible" : "Désactivée"} />
                  </td>
                  <td>{s.holder_id ? <Link to={`/personnel/${s.holder_id}`}>{s.holder_name}</Link> : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
      <Section title="Nouvelle SIM">
        <form onSubmit={create}>
          <Field label="Référence SIM (ICCID)" required>
            <input value={f.iccid} onChange={(e) => setF({ ...f, iccid: e.target.value })} required className="mono" />
          </Field>
          <div className="form-grid">
            <Field label="Numéro de ligne">
              <input value={f.line_number} onChange={(e) => setF({ ...f, line_number: e.target.value })} />
            </Field>
            <Field label="Opérateur">
              <input value={f.operator} onChange={(e) => setF({ ...f, operator: e.target.value })} />
            </Field>
          </div>
          <ErrorBox error={cmd.error} />
          <div className="form-actions">
            <button className="btn btn-primary" type="submit">
              Enregistrer
            </button>
          </div>
        </form>
      </Section>
    </div>
  );
}
