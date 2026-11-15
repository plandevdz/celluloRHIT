import { Link } from "react-router-dom";
import { api } from "../api";
import { Badge, Empty, ErrorBox, Loading, PageHeader, Section } from "../components";
import { useLoad } from "../hooks";
import { caseKindLabel, exitStateLabel, fmtDate, maintenanceLabel, roleLabel, vehicleDocLabel, vehicleStatusLabel } from "../labels";

/** États des lieux (M07), filtrés côté serveur selon les droits. */
export default function EtatsPage() {
  const d = useLoad(() => api<any>("/reports/overview"), []);
  if (d.loading && !d.data) return <Loading />;
  if (d.error) return <ErrorBox error={d.error} onRetry={d.reload} />;
  const r = d.data;
  const e = r.employees;
  return (
    <>
      <PageHeader
        title="États des lieux"
        subtitle="Situation du personnel, des actions, des accès, des biens et des échéances."
        actions={
          <a className="btn" href="/api/v1/reports/export.xlsx">
            Exporter (Excel)
          </a>
        }
      />
      <div className="stat-row">
        <div className="stat">
          <div className="value">{e.a_venir}</div>
          <div className="label">Salariés à venir</div>
        </div>
        <div className="stat">
          <div className="value">{e.actif}</div>
          <div className="label">Actifs</div>
        </div>
        <div className="stat">
          <div className="value">{e.sorti}</div>
          <div className="label">Sortis</div>
        </div>
        <div className="stat">
          <div className="value">{e.presence_a_confirmer}</div>
          <div className="label">Présences à confirmer</div>
        </div>
        {r.cases &&
          Object.entries(r.cases as Record<string, number>).map(([k, n]) => (
            <div className="stat" key={k}>
              <div className="value">{n}</div>
              <div className="label">{caseKindLabel[k]} ouverts</div>
            </div>
          ))}
      </div>
      <div className="grid-2">
        <div className="stack">
          <Section title="Tâches par équipe">
            {r.tasks.length === 0 ? (
              <Empty>Aucune tâche ouverte.</Empty>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Équipe</th>
                    <th className="num">Sans responsable</th>
                    <th className="num">En retard</th>
                  </tr>
                </thead>
                <tbody>
                  {r.tasks.map((t: any) => (
                    <tr key={t.role}>
                      <td>{roleLabel[t.role]}</td>
                      <td className="num">{t.sans_responsable}</td>
                      <td className="num">{t.en_retard}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {r.late_tasks.length > 0 && (
              <details>
                <summary>Tâches en retard ({r.late_tasks.length})</summary>
                <ul className="timeline">
                  {r.late_tasks.map((t: any, i: number) => (
                    <li key={i}>
                      <Link to={`/dossiers/${t.reference}`}>{t.reference}</Link> · {t.title} · {t.salarie} · {roleLabel[t.role]} · échéance {t.due}
                      {t.owner ? ` · ${t.owner}` : " · sans responsable"}
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </Section>
          <Section title={`Accès à couper (${r.access_to_cut.length})`}>
            {r.access_to_cut.length === 0 ? (
              <Empty>Aucun accès en attente de coupure.</Empty>
            ) : (
              <ul className="timeline">
                {r.access_to_cut.map((a: any, i: number) => (
                  <li key={i}>
                    <Link to={`/dossiers/${a.reference}`}>{a.reference}</Link> · {a.salarie} · {a.acces} · coupure {a.coupure}{" "}
                    {a.en_retard ? <Badge status="echec" label="Accès à vérifier" /> : <Badge status={a.etat} label={exitStateLabel[a.etat]} />}
                  </li>
                ))}
              </ul>
            )}
          </Section>
          <Section title={`Retours manquants (${r.returns_missing.length})`}>
            {r.returns_missing.length === 0 ? (
              <Empty>Aucun retour en attente.</Empty>
            ) : (
              <ul className="timeline">
                {r.returns_missing.map((a: any, i: number) => (
                  <li key={i}>
                    <Link to={`/dossiers/${a.reference}`}>{a.reference}</Link> · {a.salarie} · {a.bien} <Badge status={a.etat} label={exitStateLabel[a.etat]} />
                    {a.suivi && <span className="small muted"> · suivi {a.suivi}</span>}
                  </li>
                ))}
              </ul>
            )}
          </Section>
        </div>
        <div className="stack">
          <Section title={`Signatures manquantes (${r.signatures_missing.length})`}>
            {r.signatures_missing.length === 0 ? (
              <Empty>Toutes les décharges ont leur scan signé.</Empty>
            ) : (
              <ul className="timeline">
                {r.signatures_missing.slice(0, 30).map((s: any, i: number) => (
                  <li key={i}>
                    {s.mouvement} · {s.nature} · {s.salarie} · {s.date}
                    {s.dossier && (
                      <>
                        {" "}
                        · <Link to={`/dossiers/${s.dossier}`}>{s.dossier}</Link>
                      </>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Section>
          <Section title="Licences">
            {r.licenses.length === 0 ? (
              <Empty>Aucun logiciel suivi.</Empty>
            ) : (
              <table>
                <tbody>
                  {r.licenses.map((l: any, i: number) => (
                    <tr key={i}>
                      <td>{l.logiciel}</td>
                      <td className="num">
                        {l.sieges - l.occupes} libre(s) / {l.sieges}
                      </td>
                      <td className="small">
                        {l.echeance ?? "—"} {l.proche && <Badge status="a_faire" label="Échéance proche" />}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Section>
          {r.vehicles && (
            <Section title="Véhicules">
              <table>
                <tbody>
                  {r.vehicles.map((v: any) => (
                    <tr key={v.immatriculation}>
                      <td className="mono">{v.immatriculation}</td>
                      <td>
                        <Badge status={v.etat} label={vehicleStatusLabel[v.etat]} />
                      </td>
                      <td className="small">{v.titulaire ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {r.vehicle_deadlines?.length > 0 && (
                <>
                  <h3 style={{ marginTop: ".8rem" }}>Échéances</h3>
                  <ul className="timeline">
                    {r.vehicle_deadlines.map((x: any, i: number) => (
                      <li key={i}>
                        <Link to={`/vehicules/${x.vehicle_id}`}>{x.plate}</Link> · {vehicleDocLabel[x.kind] ?? maintenanceLabel[x.kind] ?? "Fin de location"} ·{" "}
                        {x.due_date ? fmtDate(x.due_date) : `${x.due_km} km`} {x.overdue && <Badge status="echec" label="Dépassée" />}
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </Section>
          )}
          {r.inventories && (
            <Section title="Écarts d'inventaire">
              {r.inventories.length === 0 ? (
                <Empty>Aucune campagne.</Empty>
              ) : (
                <table>
                  <tbody>
                    {r.inventories.map((c: any) => (
                      <tr key={c.reference}>
                        <td>
                          {c.reference} {c.nom}
                        </td>
                        <td className="small">
                          {c.non_vus} non vu(s) · {c.ailleurs} ailleurs · {c.inconnus} inconnu(s) / {c.attendus}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </Section>
          )}
        </div>
      </div>
    </>
  );
}
