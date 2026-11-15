import { Link } from "react-router-dom";
import { api } from "../api";
import { Badge, Empty, ErrorBox, Loading, PageHeader, Section } from "../components";
import { useLoad } from "../hooks";
import { caseKindLabel, fmtDate } from "../labels";

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

export default function HomePage() {
  const onb = useLoad(() => api<{ items: CaseItem[] }>("/cases?kind=ONB&status=ouvert"), []);
  const dem = useLoad(() => api<{ items: CaseItem[] }>("/cases?kind=DEM&status=ouvert"), []);
  const toCheck = useLoad(() => api<any[]>("/employees/arrivals-to-check"), []);
  const arrivals = onb.data?.items ?? [];
  const requests = dem.data?.items ?? [];
  const unowned = [...arrivals, ...requests].reduce((n, c) => n + c.unowned_tasks, 0);
  const done = arrivals.filter((c) => c.open_tasks === 0).length;

  return (
    <>
      <PageHeader
        title="Accueil RH"
        subtitle="Arrivées prévues, demandes en cours et dossiers à clôturer."
        actions={
          <Link to="/arrivees/nouvelle" className="btn btn-primary">
            + Annoncer une arrivée
          </Link>
        }
      />
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
