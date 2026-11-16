import { Link } from "react-router-dom";
import { api } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Loading, PageHeader, Section } from "../components";
import { useLoad } from "../hooks";
import { caseKindLabel, fmtDate, maintenanceLabel, vehicleDocLabel } from "../labels";

type CaseItem = {
  id: string;
  reference: string;
  kind: string;
  status: string;
  employee_id: string;
  first_name: string;
  last_name: string;
  site_name: string | null;
  effective_date: string | null;
  open_tasks: number;
  unowned_tasks: number;
};

/** Rappel à l'ADMIN : aucune sauvegarde réussie depuis 26 heures. */
function BackupReminder() {
  const runs = useLoad(() => api<any[]>("/admin/backups"), []);
  if (!runs.data) return null;
  const last = runs.data.find((r) => r.status === "reussie");
  if (last && Date.now() - new Date(last.finished_at).getTime() < 26 * 3600 * 1000) return null;
  return (
    <div className="alert alert-warn" role="status">
      {last ? "Aucune sauvegarde réussie depuis plus de 26 heures." : "Aucune sauvegarde n'a encore été faite."}{" "}
      <Link to="/administration?onglet=sauvegardes">Ouvrir les sauvegardes</Link>
    </div>
  );
}

export default function HomePage() {
  const { on, has } = useAuth();
  const onb = useLoad(() => api<{ items: CaseItem[] }>("/cases?kind=ONB&status=ouvert"), []);
  const dem = useLoad(() => api<{ items: CaseItem[] }>("/cases?kind=DEM&status=ouvert"), []);
  const toCheck = useLoad(() => api<any[]>("/employees/arrivals-to-check"), []);
  const off = useLoad(() => api<{ items: CaseItem[] }>("/cases?kind=OFF&status=ouvert"), []);
  const mut = useLoad(() => api<{ items: CaseItem[] }>("/cases?kind=MUT&status=ouvert"), []);
  const overview = useLoad(() => api<any>("/reports/overview"), []);
  const deadlines = useLoad(() => (on("vehicules") ? api<any[]>("/vehicles/deadlines") : Promise.resolve([])), [on("vehicules")]);
  const arrivals = onb.data?.items ?? [];
  const requests = dem.data?.items ?? [];
  const unowned = [...arrivals, ...requests].reduce((n, c) => n + c.unowned_tasks, 0);
  const done = arrivals.filter((c) => c.open_tasks === 0).length;

  return (
    <>
      <PageHeader
        title="Accueil RH"
        subtitle={on("depart") ? "Arrivées prévues, départs à préparer, signatures en attente et échéances véhicules." : "Arrivées prévues, demandes ponctuelles et signatures en attente."}
        actions={
          <Link to="/arrivees/nouvelle" className="btn btn-primary">
            + Annoncer une arrivée
          </Link>
        }
      />
      {has("ADMIN") && <BackupReminder />}
      <div className="stat-row">
        <div className="stat">
          <div className="value">{arrivals.length}</div>
          <div className="label">Arrivées en cours</div>
        </div>
        <div className="stat">
          <div className="value">{requests.length}</div>
          <div className="label">Demandes ponctuelles ouvertes</div>
        </div>
        <div className="stat">
          <div className="value">{unowned}</div>
          <div className="label">Tâches sans responsable</div>
        </div>
        <div className="stat">
          <div className="value">{done}</div>
          <div className="label">Arrivées prêtes à clôturer</div>
        </div>
        {on("depart") && (
          <div className="stat">
            <div className="value">{off.data?.items.length ?? 0}</div>
            <div className="label">Départs en cours</div>
          </div>
        )}
      </div>

      <div className="stack">
        {(toCheck.data?.length ?? 0) > 0 && (
          <Section title={`Présence à confirmer (${toCheck.data!.length})`}>
            <p className="small muted">
              Date prévue passée ou dossier d'arrivée clos, mais présence non confirmée : ces personnes ne sont pas comptées
              actives. Ouvrez la fiche pour confirmer la date réelle.
            </p>
            <table>
              <tbody>
                {toCheck.data!.map((e) => (
                  <tr key={e.id}>
                    <td>
                      <Link to={`/personnel/${e.id}`}>
                        {e.first_name} {e.last_name}
                      </Link>
                    </td>
                    <td>{e.site_name ?? "—"}</td>
                    <td>prévue le {fmtDate(e.planned_arrival)}</td>
                    <td>
                      {e.onb_reference && <Link to={`/dossiers/${e.onb_reference}`}>{e.onb_reference}</Link>}
                      {e.onb_closed && <span className="small muted"> (clos)</span>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Section>
        )}
        <Section title="Arrivées prévues">
          <ErrorBox error={onb.error} onRetry={onb.reload} />
          {onb.loading && !onb.data ? <Loading /> : <CaseTable items={arrivals} empty="Aucune arrivée en cours." />}
        </Section>
        <Section title="Demandes ponctuelles ouvertes">
          <ErrorBox error={dem.error} onRetry={dem.reload} />
          {dem.loading && !dem.data ? <Loading /> : <CaseTable items={requests} empty="Aucune demande ouverte." />}
        </Section>
        {on("depart") && (
          <Section title="Départs à préparer">
            {off.loading && !off.data ? <Loading /> : <CaseTable items={off.data?.items ?? []} empty="Aucun départ en cours." />}
          </Section>
        )}
        {on("mutation") && (mut.data?.items.length ?? 0) > 0 && (
          <Section title="Mutations en cours">
            <CaseTable items={mut.data!.items} empty="" />
          </Section>
        )}
        <div className="grid-2">
          <Section title={`Signatures en attente (${overview.data?.signatures_missing?.length ?? 0})`}>
            {(overview.data?.signatures_missing?.length ?? 0) === 0 ? (
              <p className="empty">Aucune décharge en attente de scan signé.</p>
            ) : (
              <ul className="timeline">
                {overview.data.signatures_missing.slice(0, 10).map((s: any, i: number) => (
                  <li key={i}>
                    {s.mouvement} · {s.salarie} · {s.date}
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
          {on("vehicules") && (
          <Section title={`Échéances véhicules (${deadlines.data?.length ?? 0})`}>
            {(deadlines.data?.length ?? 0) === 0 ? (
              <p className="empty">Aucune échéance proche.</p>
            ) : (
              <ul className="timeline">
                {deadlines.data!.slice(0, 10).map((d) => (
                  <li key={`${d.source}-${d.id}`}>
                    <Link to={`/vehicules/${d.vehicle_id}`}>{d.plate}</Link> · {vehicleDocLabel[d.kind] ?? maintenanceLabel[d.kind] ?? "Fin de location"} ·{" "}
                    {d.due_date ? fmtDate(d.due_date) : `${d.due_km} km`}
                    {d.overdue && <strong style={{ color: "var(--danger)" }}> · dépassée</strong>}
                  </li>
                ))}
              </ul>
            )}
          </Section>
          )}
        </div>
      </div>
    </>
  );
}

export function CaseTable({ items, empty }: { items: CaseItem[]; empty: string }) {
  if (items.length === 0) return <Empty>{empty}</Empty>;
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Dossier</th>
            <th>Salarié</th>
            <th>Site</th>
            <th>Date d'effet</th>
            <th>Avancement</th>
          </tr>
        </thead>
        <tbody>
          {items.map((c) => (
            <tr key={c.id}>
              <td className="nowrap">
                <Link to={`/dossiers/${c.reference}`}>
                  <strong>{c.reference}</strong>
                </Link>
                <div className="small muted">{caseKindLabel[c.kind]}</div>
              </td>
              <td>
                <Link to={`/personnel/${c.employee_id}`}>
                  {c.first_name} {c.last_name}
                </Link>
              </td>
              <td>{c.site_name ?? "—"}</td>
              <td className="nowrap">{fmtDate(c.effective_date)}</td>
              <td>
                {c.open_tasks === 0 ? (
                  <Badge status="termine" label="Rien en attente" />
                ) : (
                  <>
                    <Badge status="en_cours" label={`${c.open_tasks} action(s) ouverte(s)`} />{" "}
                    {c.unowned_tasks > 0 && <Badge status="a_faire" label={`${c.unowned_tasks} sans responsable`} />}
                  </>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
