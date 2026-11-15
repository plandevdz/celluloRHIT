import { FormEvent, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { api, ApiError } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, Notice, PageHeader, Pager, Section } from "../components";
import { useLoad } from "../hooks";
import { employeeStatusLabel, fmtDate } from "../labels";

export default function EmployeesPage() {
  const { has } = useAuth();
  const [params, setParams] = useSearchParams();
  const [q, setQ] = useState(params.get("q") ?? "");
  const status = params.get("statut") ?? "";
  const page = Number(params.get("page") ?? "1");
  const query = new URLSearchParams({ page: String(page) });
  if (params.get("q")) query.set("q", params.get("q")!);
  if (status) query.set("status", status);
  const list = useLoad(() => api<{ items: any[]; total: number; page_size: number }>(`/employees?${query}`), [params.toString()]);
  const [creating, setCreating] = useState(false);

  const setFilter = (k: string, v: string) => {
    const p = new URLSearchParams(params);
    if (v) p.set(k, v);
    else p.delete(k);
    p.delete("page");
    setParams(p, { replace: true });
  };

  return (
    <>
      <PageHeader
        title="Personnel"
        subtitle={list.data ? `${list.data.total} fiche(s)` : undefined}
        actions={
          has("RH", "ADMIN") && (
            <>
              <a className="btn" href={`/api/v1/personnel/export.xlsx${status ? `?status=${status}` : ""}`}>
                Exporter (Excel)
              </a>
              <Link className="btn" to="/personnel/import">
                Importer (Excel)
              </Link>
              <button className="btn" onClick={() => setCreating(true)}>
                Nouvelle fiche (sans parcours)
              </button>
              <Link className="btn btn-primary" to="/arrivees/nouvelle">
                + Annoncer une arrivée
              </Link>
            </>
          )
        }
      />
      <form
        className="filters"
        onSubmit={(e) => {
          e.preventDefault();
          setFilter("q", q.trim());
        }}
      >
        <label className="field">
          <span className="field-label">Recherche</span>
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Nom, prénom, matricule" />
        </label>
        <label className="field">
          <span className="field-label">Statut</span>
          <select value={status} onChange={(e) => setFilter("statut", e.target.value)}>
            <option value="">Tous</option>
            {Object.entries(employeeStatusLabel).map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
        </label>
        <button className="btn" type="submit">
          Rechercher
        </button>
      </form>
      <ErrorBox error={list.error} onRetry={list.reload} />
      <Section title="Fiches">
        {list.loading && !list.data ? (
          <Loading />
        ) : list.data?.items.length === 0 ? (
          <Empty>Aucune fiche.</Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nom</th>
                  <th>Matricule</th>
                  <th>Poste</th>
                  <th>Site</th>
                  <th>Statut</th>
                  <th>Dossiers ouverts</th>
                </tr>
              </thead>
              <tbody>
                {list.data?.items.map((e) => (
                  <tr key={e.id}>
                    <td>
                      <Link to={`/personnel/${e.id}`} dir="auto">
                        <strong>
                          {e.last_name} {e.first_name}
                        </strong>
                      </Link>
                    </td>
                    <td className="mono">{e.matricule ?? "—"}</td>
                    <td>{e.job_title ?? "—"}</td>
                    <td>{e.site_name ?? "—"}</td>
                    <td>
                      <Badge status={e.status} label={employeeStatusLabel[e.status]} />
                      {e.status === "a_venir" && <div className="small muted">le {fmtDate(e.planned_arrival)}</div>}
                    </td>
                    <td className="num">{e.open_cases}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <Pager
          page={page}
          hasNext={!!list.data && page * list.data.page_size < list.data.total}
          onPage={(p) => {
            const n = new URLSearchParams(params);
            n.set("page", String(p));
            setParams(n);
          }}
        />
      </Section>
      {creating && <CreateEmployeeDialog onClose={() => setCreating(false)} />}
    </>
  );
}

/** Fiche hors parcours : n'annonce rien, ne crée ni tâche ni email. */
function CreateEmployeeDialog({ onClose }: { onClose: () => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const navigate = useNavigate();
  const [f, setF] = useState({ first_name: "", last_name: "", matricule: "", job_title: "", site_id: "", department: "", arrival_date: "", already_present: true });
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const r = await api<{ id: string }>("/employees", { body: { ...f, matricule: f.matricule || null, department: f.department || null } });
      navigate(`/personnel/${r.id}`);
    } catch (err) {
      setError(err as ApiError);
    }
  };
  const bind = (k: keyof typeof f) => ({
    value: f[k] as string,
    onChange: (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value }),
  });
  return (
    <Modal title="Nouvelle fiche salarié" onClose={onClose}>
      <Notice kind="info">Cette fiche ne déclenche aucune annonce. Pour une nouvelle recrue, utilisez « Annoncer une arrivée ».</Notice>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="Prénom" required>
            <input {...bind("first_name")} required dir="auto" />
          </Field>
          <Field label="Nom" required>
            <input {...bind("last_name")} required dir="auto" />
          </Field>
          <Field label="Matricule">
            <input {...bind("matricule")} />
          </Field>
          <Field label="Poste" required>
            <input {...bind("job_title")} required />
          </Field>
          <Field label="Site" required>
            <select {...bind("site_id")} required>
              <option value="">Choisir…</option>
              {sites.data?.filter((s) => s.active).map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Département">
            <input {...bind("department")} />
          </Field>
          <Field label={f.already_present ? "Date d'arrivée réelle" : "Date d'arrivée prévue"} required>
            <input type="date" {...bind("arrival_date")} required />
          </Field>
        </div>
        <label className="inline-choice">
          <input type="checkbox" checked={f.already_present} onChange={(e) => setF({ ...f, already_present: e.target.checked })} />
          Salarié déjà présent dans l'entreprise
        </label>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            Créer la fiche
          </button>
        </div>
      </form>
    </Modal>
  );
}
