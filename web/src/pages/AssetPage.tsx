import { FormEvent, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../api";
import { Badge, Empty, ErrorBox, Field, LegacyProof, Loading, Modal, PageHeader, Section } from "../components";
import { useAuth } from "../auth";
import { useCommand, useLoad } from "../hooks";
import { assetConditionLabel, assetKindLabel, assetStatusLabel, fmtDate, fmtDateTime } from "../labels";
import { LabelsDialog } from "./ParcExtras";

// États modifiables à la main ; « remis » passe uniquement par une remise.
const MANUAL = ["disponible", "reserve", "en_service", "controle", "reparation", "reforme"];

const movementKind: Record<string, string> = { remise: "Remise", restitution: "Restitution", reprise: "Reprise SharePoint" };

export default function AssetPage() {
  const { id = "" } = useParams();
  const { on } = useAuth();
  const detail = useLoad(() => api<any>(`/assets/${id}`), [id]);
  const cmd = useCommand();
  const [status, setStatus] = useState("");
  const [dialog, setDialog] = useState<null | "edit" | "labels" | "location">(null);

  if (detail.loading && !detail.data) return <Loading />;
  if (detail.error) return <ErrorBox error={detail.error} onRetry={detail.reload} />;
  const { asset: a, notes, history } = detail.data;

  const changeStatus = async () => {
    const r = await cmd.run(() => api(`/assets/${a.id}`, { method: "PATCH", body: { version: a.version, status } }));
    if (r) {
      setStatus("");
      detail.reload();
    }
  };
  const done = () => {
    setDialog(null);
    detail.reload();
  };

  return (
    <>
      <PageHeader
        title={`${a.inventory_no} — ${a.label}`}
        subtitle={
          <>
            {assetKindLabel[a.kind]} · <Badge status={a.status} label={assetStatusLabel[a.status]} />
            {a.holder_name && (
              <>
                {" "}
                · détenu par <Link to={`/personnel/${a.holder_id}`}>{a.holder_name}</Link>
              </>
            )}
          </>
        }
        actions={
          <>
            {on("inventaire") && (
              <button className="btn" onClick={() => setDialog("labels")}>
                Imprimer l'étiquette
              </button>
            )}
            <button className="btn" onClick={() => setDialog("edit")}>
              Modifier
            </button>
            <Link className="btn" to="/parc">
              Retour au parc
            </Link>
          </>
        }
      />
      <div className="grid-2">
        <Section title="Identification">
          <dl className="kv">
            <dt>N° d'inventaire</dt>
            <dd className="mono">{a.inventory_no}</dd>
            <dt>Marque · modèle</dt>
            <dd>{[a.brand, a.model].filter(Boolean).join(" ") || "—"}</dd>
            <dt>N° de série</dt>
            <dd className="mono">{a.serial_no ?? "—"}</dd>
            <dt>Nom Intune</dt>
            <dd className="mono">{a.intune_name ?? "—"}</dd>
            <dt>Achat · garantie</dt>
            <dd>
              {fmtDate(a.purchase_date)} · fin de garantie {fmtDate(a.warranty_end)}
            </dd>
            <dt>État matériel</dt>
            <dd>{a.condition ? assetConditionLabel[a.condition] : "—"}</dd>
            <dt>Jeton QR</dt>
            <dd className="mono">{a.qr_token}</dd>
            <dt>Site · local attendu</dt>
            <dd>
              {a.site_name ?? "—"}
              {a.location_name && ` · ${a.location_name}`}{" "}
              <button className="btn-link small" onClick={() => setDialog("location")}>
                rectifier
              </button>
            </dd>
            {notes && (
              <>
                <dt>Notes</dt>
                <dd style={{ whiteSpace: "pre-line" }}>{notes}</dd>
              </>
            )}
          </dl>
          <p className="small muted">Le jeton QR ne contient ni nom ni site et ne donne aucun droit ; il ne change jamais, même après réimpression.</p>
        </Section>
        <Section title="Changer l'état">
          {a.status === "affecte" ? (
            <p className="muted">Actif remis : sa libération passe par une restitution depuis la fiche du détenteur.</p>
          ) : (
            <>
              <Field label="Nouvel état">
                <select value={status} onChange={(e) => setStatus(e.target.value)}>
                  <option value="">Choisir…</option>
                  {MANUAL.filter((s) => s !== a.status).map((s) => (
                    <option key={s} value={s}>
                      {assetStatusLabel[s]}
                    </option>
                  ))}
                </select>
              </Field>
              <ErrorBox error={cmd.error} />
              <button className="btn btn-primary" disabled={!status || cmd.busy} onClick={changeStatus}>
                Enregistrer
              </button>
            </>
          )}
        </Section>
      </div>
      <div style={{ marginTop: "1rem" }}>
        <Section title="Mouvements">
          {history.length === 0 ? (
            <Empty>Jamais remis.</Empty>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>Date</th>
                  <th>Mouvement</th>
                  <th>Salarié</th>
                  <th>Opérateur</th>
                  <th>État</th>
                </tr>
              </thead>
              <tbody>
                {history.map((h: any) => (
                  <tr key={h.movement_id}>
                    <td className="nowrap">{fmtDateTime(h.effective_at)}</td>
                    <td>
                      {movementKind[h.kind] ?? h.kind} {h.reference}
                      {h.note && <div className="small muted">{h.note}</div>}
                      {h.kind === "reprise" && <LegacyProof url={h.legacy_proof_url} reference={h.legacy_proof_ref} />}
                    </td>
                    <td>
                      <Link to={`/personnel/${h.employee_id}`}>{h.employee}</Link>
                    </td>
                    <td>{h.operator}</td>
                    <td>{h.condition ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Section>
      </div>
      {dialog === "labels" && <LabelsDialog assets={[{ id: a.id, inventory_no: a.inventory_no }]} onClose={() => setDialog(null)} />}
      {dialog === "edit" && <EditAssetDialog asset={a} notes={notes} onClose={() => setDialog(null)} onDone={done} />}
      {dialog === "location" && <LocationDialog asset={a} onClose={() => setDialog(null)} onDone={done} />}
    </>
  );
}

function EditAssetDialog({ asset, notes, onClose, onDone }: { asset: any; notes: string | null; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({
    label: asset.label ?? "",
    brand: asset.brand ?? "",
    model: asset.model ?? "",
    serial_no: asset.serial_no ?? "",
    intune_name: asset.intune_name ?? "",
    purchase_date: asset.purchase_date ?? "",
    warranty_end: asset.warranty_end ?? "",
    condition: asset.condition ?? "",
    notes: notes ?? "",
  });
  const [error, setError] = useState<ApiError | null>(null);
  const bind = (k: keyof typeof f) => ({ value: f[k], onChange: (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value }) });
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api(`/assets/${asset.id}`, {
        method: "PATCH",
        body: Object.fromEntries([["version", asset.version], ...Object.entries(f).map(([k, v]) => [k, v === "" ? null : v])]),
      });
      onDone();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={`Modifier ${asset.inventory_no}`} onClose={onClose} wide>
      <form onSubmit={submit}>
        <Field label="Libellé" required>
          <input {...bind("label")} required />
        </Field>
        <div className="form-grid">
          <Field label="Marque">
            <input {...bind("brand")} />
          </Field>
          <Field label="Modèle">
            <input {...bind("model")} />
          </Field>
          <Field label="N° de série">
            <input {...bind("serial_no")} className="mono" />
          </Field>
          <Field label="Nom Intune">
            <input {...bind("intune_name")} className="mono" />
          </Field>
          <Field label="Date d'achat">
            <input type="date" {...bind("purchase_date")} />
          </Field>
          <Field label="Fin de garantie">
            <input type="date" {...bind("warranty_end")} />
          </Field>
          <Field label="État matériel">
            <select {...bind("condition")}>
              <option value="">—</option>
              {Object.entries(assetConditionLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <Field label="Notes">
          <textarea rows={3} {...bind("notes")} />
        </Field>
        <p className="small muted">Un champ vidé conserve la valeur actuelle.</p>
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

/** Rectification de l'emplacement : commande distincte du pointage, motivée. */
function LocationDialog({ asset, onClose, onDone }: { asset: any; onClose: () => void; onDone: () => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [site, setSite] = useState(asset.site_id ?? "");
  const [loc, setLoc] = useState(asset.location_id ?? "");
  const [reason, setReason] = useState("");
  const cmd = useCommand();
  const current = sites.data?.find((s) => s.id === site);
  return (
    <Modal title={`Rectifier l'emplacement de ${asset.inventory_no}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/assets/${asset.id}`, { method: "PATCH", body: { version: asset.version, site_id: site || null, location_id: loc || null, reason } }),
          );
          if (r) onDone();
        }}
      >
        <div className="form-grid">
          <Field label="Site">
            <select value={site} onChange={(e) => { setSite(e.target.value); setLoc(""); }}>
              <option value="">—</option>
              {sites.data?.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Local">
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
        <Field label="Motif (ex. constat d'inventaire INV-00003)" required>
          <input value={reason} onChange={(e) => setReason(e.target.value)} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy || !reason.trim()}>
            Rectifier le parc
          </button>
        </div>
      </form>
    </Modal>
  );
}
