import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../api";
import { Badge, Empty, ErrorBox, Field, Loading, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import { assetKindLabel, assetStatusLabel, fmtDateTime } from "../labels";

// États modifiables à la main ; « remis » passe uniquement par une remise.
const MANUAL = ["disponible", "reserve", "controle", "reparation", "reforme"];

export default function AssetPage() {
  const { id = "" } = useParams();
  const detail = useLoad(() => api<any>(`/assets/${id}`), [id]);
  const cmd = useCommand();
  const [status, setStatus] = useState("");

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
        actions={<Link className="btn" to="/parc">Retour au parc</Link>}
      />
      <div className="grid-2">
        <Section title="Identification">
          <dl className="kv">
            <dt>N° d'inventaire</dt>
            <dd className="mono">{a.inventory_no}</dd>
            <dt>N° de série</dt>
            <dd className="mono">{a.serial_no ?? "—"}</dd>
            <dt>Jeton QR</dt>
            <dd className="mono">{a.qr_token}</dd>
            <dt>Site · local attendu</dt>
            <dd>
              {a.site_name ?? "—"}
              {a.location_name && ` · ${a.location_name}`}
            </dd>
            {notes && (
              <>
                <dt>Notes</dt>
                <dd>{notes}</dd>
              </>
            )}
          </dl>
          <p className="small muted">Le jeton QR ne contient ni nom ni site et ne donne aucun droit. L'impression des étiquettes arrive dans la tranche suivante.</p>
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
                      {h.kind === "remise" ? "Remise" : "Restitution"} {h.reference}
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
    </>
  );
}
