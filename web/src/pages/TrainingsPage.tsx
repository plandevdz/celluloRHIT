import { FormEvent, useState } from "react";
import { Link } from "react-router-dom";
import { api, ApiError } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import { fmtDate, sessionStatusLabel, todayIso } from "../labels";

type Action = "planifier" | "reporter" | "realiser" | "dispenser" | "annuler";
const actionLabel: Record<Action, string> = {
  planifier: "Planifier",
  reporter: "Reporter",
  realiser: "Marquer réalisée",
  dispenser: "Dispenser",
  annuler: "Annuler",
};

/** Sessions de mon équipe : disponibles dès l'annonce, indépendantes du compte mail. */
export default function TrainingsPage() {
  const { has } = useAuth();
  const [scope, setScope] = useState("team");
  const [status, setStatus] = useState("open");
  const sessions = useLoad(() => api<any[]>(`/trainings/sessions?scope=${scope}&status=${status}`), [scope, status]);
  const [dialog, setDialog] = useState<{ session: any; action: Action } | null>(null);
  const canAct = has("FORMATION", "ADMIN");

  return (
    <>
      <PageHeader title="Formations" subtitle="Sessions à planifier, reportées ou réalisées." />
      <div className="filters">
        <label className="field">
          <span className="field-label">Vue</span>
          <select value={scope} onChange={(e) => setScope(e.target.value)}>
            <option value="team">Toute l'équipe</option>
            <option value="mine">Mes sessions</option>
          </select>
        </label>
        <label className="field">
          <span className="field-label">État</span>
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="open">À traiter</option>
            <option value="done">Soldées</option>
          </select>
        </label>
      </div>
      <div className="grid-2" style={{ gridTemplateColumns: "2fr 1fr" }}>
        <Section title="Sessions">
          <ErrorBox error={sessions.error} onRetry={sessions.reload} />
          {sessions.loading && !sessions.data ? (
            <Loading />
          ) : sessions.data?.length === 0 ? (
            <Empty>Aucune session.</Empty>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Formation</th>
                    <th>Salarié · site</th>
                    <th>Intervenant</th>
                    <th>Échéance</th>
                    <th>État</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {sessions.data?.map((s) => (
                    <tr key={s.id}>
                      <td>
                        <strong>{s.name}</strong>
                        {s.case_reference && (
                          <div className="small">
                            <Link to={`/dossiers/${s.case_reference}`}>{s.case_reference}</Link>
                          </div>
                        )}
                      </td>
                      <td>
                        {s.employee_name}
                        <div className="small muted">{s.site_name ?? "—"}</div>
                      </td>
                      <td>{s.trainer ?? <span className="muted">File de l'équipe</span>}</td>
                      <td className="nowrap">{fmtDate(s.due_date)}</td>
                      <td>
                        <Badge status={s.status} label={sessionStatusLabel[s.status]} />
                        {s.planned_date && <div className="small">prévue le {fmtDate(s.planned_date)}</div>}
                        {s.done_date && <div className="small">réalisée le {fmtDate(s.done_date)}</div>}
                        {s.reason && <div className="small">Motif : {s.reason}</div>}
                      </td>
                      <td>
                        {status === "open" && (
                          <div className="row">
                            {canAct &&
                              (["planifier", "reporter", "realiser", "dispenser"] as Action[]).map((a) => (
                                <button key={a} className={`btn btn-small ${a === "realiser" ? "btn-primary" : ""}`} onClick={() => setDialog({ session: s, action: a })}>
                                  {actionLabel[a]}
                                </button>
                              ))}
                            {has("RH", "ADMIN") && (
                              <button className="btn btn-small btn-danger" onClick={() => setDialog({ session: s, action: "annuler" })}>
                                Annuler
                              </button>
                            )}
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
        <CatalogSection canEdit={canAct} />
      </div>
      {dialog && (
        <SessionDialog
          {...dialog}
          onClose={() => setDialog(null)}
          onDone={() => {
            setDialog(null);
            sessions.reload();
          }}
        />
      )}
    </>
  );
}

function SessionDialog({ session, action, onClose, onDone }: { session: any; action: Action; onClose: () => void; onDone: () => void }) {
  const trainers = useLoad(() => api<any[]>("/users/directory?role=FORMATION"), []);
  const [date, setDate] = useState(action === "realiser" ? todayIso() : session.planned_date ?? "");
  const [reason, setReason] = useState("");
  const [result, setResult] = useState("");
  const [trainer, setTrainer] = useState(session.trainer_id ?? "");
  const [error, setError] = useState<ApiError | null>(null);
  const needsDate = action === "planifier" || action === "realiser" || action === "reporter";
  const needsReason = action === "reporter" || action === "dispenser" || action === "annuler";
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api(`/trainings/sessions/${session.id}`, {
        body: { action, date: date || null, reason: reason || null, result: result || null, trainer_id: trainer || null },
      });
      onDone();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={`${actionLabel[action]} — ${session.name} (${session.employee_name})`} onClose={onClose}>
      <form onSubmit={submit}>
        {needsDate && (
          <Field label={action === "realiser" ? "Date réelle" : action === "reporter" ? "Nouvelle date prévue" : "Date prévue"} required={action === "planifier"}>
            <input type="date" value={date} onChange={(e) => setDate(e.target.value)} required={action === "planifier"} />
          </Field>
        )}
        {action !== "annuler" && (
          <Field label="Intervenant">
            <select value={trainer} onChange={(e) => setTrainer(e.target.value)}>
              <option value="">Moi-même</option>
              {trainers.data?.map((u) => (
                <option key={u.id} value={u.id}>
                  {u.display_name}
                </option>
              ))}
            </select>
          </Field>
        )}
        {needsReason && (
          <Field label="Motif" required>
            <textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} required />
          </Field>
        )}
        {action === "realiser" && (
          <Field label="Résultat / observation">
            <input value={result} onChange={(e) => setResult(e.target.value)} />
          </Field>
        )}
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Fermer
          </button>
          <button type="submit" className="btn btn-primary">
            {actionLabel[action]}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function CatalogSection({ canEdit }: { canEdit: boolean }) {
  const catalog = useLoad(() => api<any[]>("/trainings/catalog"), []);
  const [name, setName] = useState("");
  const cmd = useCommand();
  return (
    <Section title="Catalogue">
      <ul className="timeline">
        {catalog.data?.map((t) => (
          <li key={t.id}>{t.name}</li>
        ))}
      </ul>
      {canEdit && (
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            const r = await cmd.run(() => api("/trainings/catalog", { body: { name } }));
            if (r) {
              setName("");
              catalog.reload();
            }
          }}
          style={{ marginTop: ".75rem" }}
        >
          <Field label="Nouvelle formation">
            <input value={name} onChange={(e) => setName(e.target.value)} required />
          </Field>
          <ErrorBox error={cmd.error} />
          <button className="btn" type="submit">
            Ajouter au catalogue
          </button>
        </form>
      )}
    </Section>
  );
}
