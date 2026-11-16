import { FormEvent, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { api, ApiError } from "../api";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, PageHeader, Section } from "../components";
import { useAuth } from "../auth";
import { useCommand, useLoad } from "../hooks";
import { assetConditionLabel, assetKindLabel, assetStatusLabel } from "../labels";
import { InventoriesTab, LabelsDialog, LinesSection, SoftwareTab } from "./ParcExtras";

const TABS = [
  { id: "actifs", label: "Actifs" },
  { id: "accessoires", label: "Accessoires" },
  { id: "sim", label: "SIM et lignes" },
  { id: "logiciels", label: "Logiciels et sièges", module: "logiciels" },
  { id: "inventaires", label: "Inventaires QR", module: "inventaire" },
];

export default function AssetsPage() {
  const [params, setParams] = useSearchParams();
  const { on } = useAuth();
  const tabs = TABS.filter((t) => !t.module || on(t.module));
  const asked = params.get("onglet") ?? "actifs";
  const tab = tabs.some((t) => t.id === asked) ? asked : "actifs";
  return (
    <>
      <PageHeader title="Parc IT" subtitle={`Disponibles, remis, en contrôle ; accessoires ; SIM et lignes${on("logiciels") ? " ; logiciels" : ""}${on("inventaire") ? " ; inventaires" : ""}.`} />
      <div className="row" role="tablist" style={{ marginBottom: "1rem" }}>
        {tabs.map((t) => (
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
      {tab === "logiciels" && <SoftwareTab />}
      {tab === "inventaires" && <InventoriesTab />}
    </>
  );
}

function AssetsTab() {
  // Étiquettes QR fermées pendant le pilote ; la saisie ou le scan d'un code
  // pour retrouver un bien reste disponible.
  const labelsOn = useAuth().on("inventaire");
  const navigate = useNavigate();
  const [filters, setFilters] = useState({ q: "", status: "", kind: "", incomplet: "" });
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
  const [selected, setSelected] = useState<string[]>([]);
  const [printing, setPrinting] = useState(false);
  const [moving, setMoving] = useState(false);

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
      <Section
        title={`Actifs (${list.data?.length ?? 0})`}
        actions={
          <>
            <button className="btn" disabled={selected.length === 0} onClick={() => setMoving(true)}>
              Changer le site ({selected.length})
            </button>
            {labelsOn && (
              <button className="btn" disabled={selected.length === 0} onClick={() => setPrinting(true)}>
                Imprimer les étiquettes ({selected.length})
              </button>
            )}
          </>
        }
      >
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
          <label className="field">
            <span className="field-label">Fiches</span>
            <select value={filters.incomplet} onChange={(e) => applyNow({ ...filters, incomplet: e.target.value })}>
              <option value="">Toutes</option>
              <option value="true">À compléter</option>
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
                  <th>
                    <input
                      type="checkbox"
                      aria-label="Tout sélectionner"
                      checked={!!list.data?.length && selected.length === list.data.length}
                      onChange={(e) => setSelected(e.target.checked ? list.data!.map((a) => a.id) : [])}
                    />
                  </th>
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
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Sélectionner ${a.inventory_no}`}
                        checked={selected.includes(a.id)}
                        onChange={(e) => setSelected(e.target.checked ? [...selected, a.id] : selected.filter((x) => x !== a.id))}
                      />
                    </td>
                    <td className="mono">
                      <Link to={`/parc/actifs/${a.id}`}>{a.inventory_no}</Link>
                    </td>
                    <td>
                      {a.label}
                      {(a.brand || a.model) && <div className="small muted">{[a.brand, a.model].filter(Boolean).join(" ")}</div>}
                    </td>
                    <td>{assetKindLabel[a.kind]}</td>
                    <td className="mono small">{a.serial_no ?? "—"}</td>
                    <td>
                      <Badge status={a.status} label={assetStatusLabel[a.status]} />
                    </td>
                    <td>{a.holder_id ? <Link to={`/personnel/${a.holder_id}`}>{a.holder_name}</Link> : "—"}</td>
                    <td className="small">
                      {a.site_name ?? "—"}
                      {a.location_name && ` · ${a.location_name}`}
                      {a.missing?.length > 0 && (
                        <div>
                          <span className="tag">à compléter : {a.missing.join(", ")}</span>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Section>
      {creating && <CreateAssetDialog onClose={() => setCreating(false)} onCreated={(id) => navigate(`/parc/actifs/${id}`)} />}
      {moving && (
        <BulkSiteDialog
          assets={(list.data ?? []).filter((a) => selected.includes(a.id))}
          onClose={() => setMoving(false)}
          onDone={() => {
            setMoving(false);
            setSelected([]);
            list.reload();
          }}
        />
      )}
      {printing && (
        <LabelsDialog
          assets={(list.data ?? []).filter((a) => selected.includes(a.id)).map((a) => ({ id: a.id, inventory_no: a.inventory_no }))}
          onClose={() => setPrinting(false)}
        />
      )}
    </div>
  );
}

function CreateAssetDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({
    inventory_no: "", kind: "pc_portable", label: "", serial_no: "", site_id: "", location_id: "", notes: "",
    brand: "", model: "", intune_name: "", purchase_date: "", warranty_end: "", condition: "neuf",
  });
  const [inService, setInService] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const site = sites.data?.find((s) => s.id === f.site_id);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      const r = await api<{ id: string }>("/assets", {
        body: {
          ...Object.fromEntries(Object.entries(f).map(([k, v]) => [k, v === "" ? null : v])),
          inventory_no: f.inventory_no,
          kind: f.kind,
          label: f.label,
          in_service: inService,
        },
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
        <div className="form-grid">
          <Field label="Marque">
            <input {...bind("brand")} />
          </Field>
          <Field label="Modèle">
            <input {...bind("model")} />
          </Field>
          <Field label="N° de série">
            <input {...bind("serial_no")} />
          </Field>
          <Field label="Nom Intune">
            <input {...bind("intune_name")} />
          </Field>
          <Field label="Date d'achat">
            <input type="date" {...bind("purchase_date")} />
          </Field>
          <Field label="Fin de garantie">
            <input type="date" {...bind("warranty_end")} />
          </Field>
          <Field label="État matériel">
            <select {...bind("condition")}>
              {Object.entries(assetConditionLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <label className="inline-choice" style={{ marginBottom: ".8rem" }}>
          <input type="checkbox" checked={inService} onChange={(e) => setInService(e.target.checked)} /> Matériel partagé mis en service
          (serveur, imprimante, réseau…), non remis à une personne
        </label>
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
                    </button>{" "}
                    <button
                      className="btn btn-small"
                      onClick={async () => {
                        const label = window.prompt("Libellé de l'accessoire", a.label);
                        if (label && label.trim() && label !== a.label && (await cmd.run(() => api(`/accessories/${a.id}`, { method: "PATCH", body: { label } }))))
                          list.reload();
                      }}
                    >
                      Renommer
                    </button>{" "}
                    <select
                      aria-label={`Site de ${a.label}`}
                      value={a.site_id ?? ""}
                      onChange={async (e) => {
                        const v = e.target.value;
                        if (await cmd.run(() => api(`/accessories/${a.id}`, { method: "PATCH", body: v ? { site_id: v } : { clear_site: true } }))) list.reload();
                      }}
                    >
                      <option value="">Site : —</option>
                      {sites.data?.map((s) => (
                        <option key={s.id} value={s.id}>
                          {s.name}
                        </option>
                      ))}
                    </select>
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
  return (
    <div className="stack">
      <SimsBlock />
      <LinesSection />
    </div>
  );
}

function SimsBlock() {
  const list = useLoad(() => api<any[]>("/sims"), []);
  const [f, setF] = useState({ iccid: "", line_number: "", operator: "", plan: "", line_type: "" });
  const cmd = useCommand();
  const create = async (e: FormEvent) => {
    e.preventDefault();
    const r = await cmd.run(() =>
      api("/sims", {
        body: { iccid: f.iccid, line_number: f.line_number || null, operator: f.operator || null, plan: f.plan || null, line_type: f.line_type || null },
      }),
    );
    if (r) {
      setF({ iccid: "", line_number: "", operator: "", plan: "", line_type: "" });
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
                <th></th>
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
                  <td>
                    <button
                      className="btn btn-small"
                      onClick={async () => {
                        const iccid = window.prompt("Référence de la SIM (ICCID)", s.iccid);
                        if (iccid && iccid.trim() && iccid !== s.iccid) {
                          try {
                            await api(`/sims/${s.id}`, { method: "PATCH", body: { iccid } });
                            list.reload();
                          } catch (err) {
                            window.alert((err as ApiError).message);
                          }
                        }
                      }}
                    >
                      Corriger
                    </button>
                  </td>
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
              <select value={f.operator} onChange={(e) => setF({ ...f, operator: e.target.value })}>
                <option value="">—</option>
                <option>Mobilis</option>
                <option>Djezzy</option>
                <option>Ooredoo</option>
              </select>
            </Field>
            <Field label="Forfait">
              <input value={f.plan} onChange={(e) => setF({ ...f, plan: e.target.value })} />
            </Field>
            <Field label="Type de ligne">
              <select value={f.line_type} onChange={(e) => setF({ ...f, line_type: e.target.value })}>
                <option value="">—</option>
                <option value="voix">Voix</option>
                <option value="data">Data</option>
                <option value="voix_data">Voix + Data</option>
              </select>
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

/** Site (et local) de plusieurs actifs à la fois : utile après l'import des
 *  listes, qui n'indiquaient pas l'emplacement du matériel. */
function BulkSiteDialog({ assets, onClose, onDone }: { assets: any[]; onClose: () => void; onDone: () => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [site, setSite] = useState("");
  const [loc, setLoc] = useState("");
  const [progress, setProgress] = useState<{ done: number; failed: string[] } | null>(null);
  const current = sites.data?.find((s) => s.id === site);
  const run = async (e: FormEvent) => {
    e.preventDefault();
    const failed: string[] = [];
    let done = 0;
    for (const a of assets) {
      try {
        await api(`/assets/${a.id}`, { method: "PATCH", body: { version: a.version, site_id: site, location_id: loc || null, reason: "Site renseigné en lot" } });
        done += 1;
      } catch (err) {
        failed.push(`${a.inventory_no} : ${(err as ApiError).message}`);
      }
      setProgress({ done, failed: [...failed] });
    }
    if (failed.length === 0) onDone();
  };
  return (
    <Modal title={`Changer le site de ${assets.length} actif(s)`} onClose={onClose}>
      <form onSubmit={run}>
        <div className="form-grid">
          <Field label="Site" required>
            <select value={site} onChange={(e) => { setSite(e.target.value); setLoc(""); }} required>
              <option value="">—</option>
              {sites.data?.filter((s) => s.active).map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Local (facultatif)">
            <select value={loc} onChange={(e) => setLoc(e.target.value)} disabled={!current}>
              <option value="">—</option>
              {current?.locations.map((l: any) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
        {progress && (
          <p className="small">
            {progress.done} / {assets.length} mis à jour
            {progress.failed.length > 0 && (
              <span style={{ color: "var(--danger)" }}>
                {" "}
                · {progress.failed.length} en échec : {progress.failed.slice(0, 5).join(" ; ")}
              </span>
            )}
          </p>
        )}
        <div className="form-actions">
          <button type="button" className="btn" onClick={progress?.failed.length ? onDone : onClose}>
            {progress?.failed.length ? "Fermer" : "Annuler"}
          </button>
          <button type="submit" className="btn btn-primary" disabled={!site || (!!progress && !progress.failed.length)}>
            Appliquer
          </button>
        </div>
      </form>
    </Modal>
  );
}
