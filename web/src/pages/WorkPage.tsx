import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Loading, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import { caseKindLabel, fmtDate, fmtDateTime, preferenceLabel, roleLabel, taskStatusLabel } from "../labels";
import { CaseTable } from "./HomePage";

export type Task = {
  id: string;
  title: string;
  role: string;
  status: string;
  owner_id: string | null;
  owner_name: string | null;
  due_date: string | null;
  blocked_reason: string | null;
  claimed_at: string | null;
  completed_at: string | null;
  case_id: string;
  case_reference: string;
  case_kind: string;
  employee_id: string;
  employee_name: string;
  site_name: string | null;
  need_category: string | null;
  need_preference: string | null;
};

/** « Mon travail » : toutes les files de mes habilitations, filtrables. */
export default function WorkPage() {
  const { user, has } = useAuth();
  const [params, setParams] = useSearchParams();
  const scope = params.get("vue") ?? "equipe";
  const role = params.get("equipe") ?? "";
  const status = params.get("statut") ?? "open";
  const q = new URLSearchParams({ scope: scope === "moi" ? "mine" : "team", status });
  if (role) q.set("role", role);
  const tasks = useLoad(() => api<Task[]>(`/tasks?${q}`), [scope, role, status]);
  const announced = useLoad(
    () => (has("IT", "FORMATION") ? api<{ items: any[] }>("/cases?kind=ONB&status=ouvert") : Promise.resolve({ items: [] })),
    [],
  );
  const cmd = useCommand();
  const [message, setMessage] = useState<string | null>(null);

  const setFilter = (k: string, v: string) => {
    const p = new URLSearchParams(params);
    if (v) p.set(k, v);
    else p.delete(k);
    setParams(p, { replace: true });
  };

  const claim = async (t: Task) => {
    setMessage(null);
    const r = await cmd.run(() => api(`/tasks/${t.id}/claim`, { body: {} }));
    if (r) setMessage(`Vous avez pris en charge « ${t.title} » (${t.case_reference}).`);
    tasks.reload();
    announced.reload();
  };

  return (
    <>
      <PageHeader title="Mon travail" subtitle={`Files : ${user?.roles.map((r) => roleLabel[r]).join(", ")}`} />
      <div className="filters">
        <label className="field">
          <span className="field-label">Vue</span>
          <select value={scope} onChange={(e) => setFilter("vue", e.target.value === "equipe" ? "" : e.target.value)}>
            <option value="equipe">Files de mes équipes</option>
            <option value="moi">Mes tâches prises</option>
          </select>
        </label>
        {(user?.roles.length ?? 0) > 1 && (
          <label className="field">
            <span className="field-label">Équipe</span>
            <select value={role} onChange={(e) => setFilter("equipe", e.target.value)}>
              <option value="">Toutes mes équipes</option>
              {user?.roles.map((r) => (
                <option key={r} value={r}>
                  {roleLabel[r]}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="field">
          <span className="field-label">État</span>
          <select value={status} onChange={(e) => setFilter("statut", e.target.value === "open" ? "" : e.target.value)}>
            <option value="open">À faire et en cours</option>
            <option value="a_faire">À prendre</option>
            <option value="bloque">En attente</option>
            <option value="done">Terminées / annulées</option>
          </select>
        </label>
        <button className="btn" onClick={tasks.reload}>
          Actualiser
        </button>
      </div>
      {message && <div className="alert alert-ok" role="status">{message}</div>}
      <ErrorBox error={cmd.error} />
      <ErrorBox error={tasks.error} onRetry={tasks.reload} />

      <Section title={`Tâches (${tasks.data?.length ?? 0})`}>
        {tasks.loading && !tasks.data ? (
          <Loading />
        ) : tasks.data?.length === 0 ? (
          <Empty>Rien à traiter dans cette vue.</Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Équipe</th>
                  <th>Dossier</th>
                  <th>Salarié · site</th>
                  <th>Action</th>
                  <th>Échéance</th>
                  <th>Prise en charge</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {tasks.data?.map((t) => (
                  <tr key={t.id}>
                    <td>{roleLabel[t.role]}</td>
                    <td className="nowrap">
                      <Link to={`/dossiers/${t.case_reference}`}>{t.case_reference}</Link>
                      <div className="small muted">{caseKindLabel[t.case_kind]}</div>
                    </td>
                    <td>
                      {t.employee_name}
                      <div className="small muted">{t.site_name ?? "—"}</div>
                    </td>
                    <td>
                      {t.title}
                      {t.need_preference && <div className="small muted">Demandé : {preferenceLabel[t.need_preference]}</div>}
                    </td>
                    <td className="nowrap">{fmtDate(t.due_date)}</td>
                    <td>
                      <Badge status={t.status} label={taskStatusLabel[t.status]} />
                      {t.owner_name && <div className="small">{t.owner_name}</div>}
                      {t.status === "bloque" && t.blocked_reason && <div className="small">Raison : {t.blocked_reason}</div>}
                      {t.completed_at && <div className="small muted">{fmtDateTime(t.completed_at)}</div>}
                    </td>
                    <td className="nowrap">
                      {t.status === "a_faire" && (
                        <button className="btn btn-primary btn-small" disabled={cmd.busy} onClick={() => claim(t)}>
                          Je prends en charge
                        </button>
                      )}{" "}
                      <Link className="btn btn-small" to={`/dossiers/${t.case_reference}`}>
                        Ouvrir
                      </Link>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Section>

      {has("IT", "FORMATION") && (
        <div style={{ marginTop: "1rem" }}>
          <Section title="Arrivées annoncées">
            <p className="small muted">
              Toutes les arrivées en cours, même sans besoin précis : demandez une précision au RH si nécessaire.
            </p>
            {announced.data && <CaseTable items={announced.data.items} empty="Aucune arrivée en cours." />}
          </Section>
        </div>
      )}
    </>
  );
}
