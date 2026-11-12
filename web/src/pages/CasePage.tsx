import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../api";
import { useAuth } from "../auth";
import { Badge, Empty, ErrorBox, Field, Loading, Modal, Notice, PageHeader, ReasonDialog, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import {
  answerLabel,
  caseKindLabel,
  caseStatusLabel,
  deliveryStatusLabel,
  employeeStatusLabel,
  fmtDate,
  fmtDateTime,
  needLabel,
  preferenceLabel,
  resolutionLabel,
  roleLabel,
  sessionStatusLabel,
  taskStatusLabel,
  visibilityText,
} from "../labels";
import { CompleteTaskDialog, ConfirmArrivalDialog } from "./TaskDialogs";
import { DocumentActions } from "./DocumentActions";
import { emptyNeeds, NeedsEditor, needsPayload, NeedValue, TrainingValue } from "./NeedsEditor";

type CaseDetail = {
  case: any;
  needs: any[];
  tasks: any[];
  trainings: any[];
  movements: any[];
  children: any[];
  history: any[];
  notifications: any[];
  scope: "complet" | "equipe";
  mail_mode: string;
};

export default function CasePage() {
  const { reference = "" } = useParams();
  const { user, has } = useAuth();
  const detail = useLoad(() => api<CaseDetail>(`/cases/${encodeURIComponent(reference)}`), [reference]);
  const cmd = useCommand();
  const [dialog, setDialog] = useState<
    null | { kind: "block" | "complete"; task: any } | { kind: "needs" } | { kind: "close" } | { kind: "arrival" }
  >(null);

  if (detail.loading && !detail.data) return <Loading />;
  if (detail.error) return <ErrorBox error={detail.error} onRetry={detail.reload} />;
  if (!detail.data) return null;
  const { case: c, needs, tasks, trainings, movements, children, history, notifications, scope, mail_mode } = detail.data;
  const isHr = has("RH", "ADMIN");
  const isOpen = c.status === "ouvert";

  const act = async (path: string, body: object = {}) => {
    const r = await cmd.run(() => api(path, { body }));
    detail.reload();
    return r;
  };

  return (
    <>
      <PageHeader
        title={
          <>
            {c.reference} — {c.first_name} {c.last_name}
          </>
        }
        subtitle={
          <>
            {caseKindLabel[c.kind]} · <Badge status={c.status} label={caseStatusLabel[c.status]} /> · {c.job_title ?? "poste ?"} ·{" "}
            {c.site_name ?? "site ?"} · date d'effet {fmtDate(c.effective_date)}
            {c.parent_reference && (
              <>
                {" "}
                · lié à <Link to={`/dossiers/${c.parent_reference}`}>{c.parent_reference}</Link>
              </>
            )}
          </>
        }
        actions={
          <>
            <Link className="btn" to={`/personnel/${c.employee_id}`}>
              Fiche salarié
            </Link>
            {isHr && c.kind === "ONB" && c.employee_status === "a_venir" && (
              <button className="btn btn-primary" onClick={() => setDialog({ kind: "arrival" })}>
                Confirmer l'arrivée
              </button>
            )}
            {isHr && isOpen && (
              <>
                <button className="btn" onClick={() => setDialog({ kind: "needs" })}>
                  Compléter la demande
                </button>
                <button className="btn" onClick={() => setDialog({ kind: "close" })}>
                  Clôturer
                </button>
              </>
            )}
          </>
        }
      />
      <ErrorBox error={cmd.error} />
      {isHr && c.note && <Notice kind="info">Note RH : {c.note}</Notice>}
      {c.close_note && <Notice kind="info">Clôture : {c.close_note}</Notice>}
      {scope === "equipe" && (
        <p className="small muted">Vue limitée aux éléments utiles à votre équipe.</p>
      )}

      <div className="grid-2">
        <div className="stack">
          <Section title="Demandé">
            {needs.length === 0 ? (
              <Empty>Aucun besoin visible pour vos équipes.</Empty>
            ) : (
              <table>
                <tbody>
                  {needs.map((n) => (
                    <tr key={n.id}>
                      <td>
                        <strong>{needLabel[n.category]}</strong>
                        {n.history.length > 1 && (
                          <details>
                            <summary className="small">{n.history.length} révisions</summary>
                            <ul className="timeline">
                              {n.history.map((h: any) => (
                                <li key={h.revision}>
                                  v{h.revision} · {answerLabel[h.answer]}
                                  {h.preference && ` · ${preferenceLabel[h.preference]}`}
                                  {h.detail && ` · ${h.detail}`} <span className="muted">({fmtDateTime(h.changed_at)})</span>
                                </li>
                              ))}
                            </ul>
                          </details>
                        )}
                      </td>
                      <td>
                        <Badge status={n.answer} label={answerLabel[n.answer]} />
                      </td>
                      <td className="small">
                        {n.preference && <div>Demandé : {preferenceLabel[n.preference]}</div>}
                        {n.quantity && <div>Quantité : {n.quantity}</div>}
                        {n.detail && <div>{n.detail}</div>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Section>

          {(isHr || has("IT")) && (
          <Section title="Remis et restitué">
            {movements.length === 0 ? (
              <Empty>Aucun mouvement de matériel pour ce dossier.</Empty>
            ) : (
              movements.map((m) => (
                <div key={m.id} className="card" style={{ marginBottom: ".6rem" }}>
                  <div className="row" style={{ justifyContent: "space-between" }}>
                    <strong>
                      {m.kind === "remise" ? "Remis" : "Restitué"} · {m.reference}
                    </strong>
                    <span className="small muted">
                      {fmtDateTime(m.effective_at)} · {m.operator}
                    </span>
                  </div>
                  <ul>
                    {m.lines?.map((l: any, i: number) => (
                      <li key={i}>
                        {l.label} {l.identifier && <span className="mono small">{l.identifier}</span>}
                        {l.quantity > 1 && ` × ${l.quantity}`}
                        {l.condition && <span className="muted"> — {l.condition}</span>}
                      </li>
                    ))}
                  </ul>
                  {m.substitution_reason && <p className="small">Substitution : {m.substitution_reason}</p>}
                  {m.document_id && <DocumentActions documentId={m.document_id} status={m.document_status} onChange={detail.reload} />}
                </div>
              ))
            )}
          </Section>
          )}

          {trainings.length > 0 && (
            <Section title="Formations" actions={<Link to="/formations">Ouvrir les formations</Link>}>
              <table>
                <tbody>
                  {trainings.map((t) => (
                    <tr key={t.id}>
                      <td>{t.name}</td>
                      <td>
                        <Badge status={t.status} label={sessionStatusLabel[t.status]} />
                      </td>
                      <td className="small">
                        {t.trainer ?? "File Formation"}
                        {t.planned_date && <div>Prévue le {fmtDate(t.planned_date)}</div>}
                        {t.done_date && <div>Réalisée le {fmtDate(t.done_date)}</div>}
                        {t.reason && <div>Motif : {t.reason}</div>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Section>
          )}
        </div>

        <div className="stack">
          <Section title="Actions des équipes">
            {tasks.length === 0 ? (
              <Empty>Aucune action générée (besoins non confirmés).</Empty>
            ) : (
              <table>
                <tbody>
                  {tasks.map((t) => {
                    const inTeam = user?.roles.includes(t.role);
                    const mine = t.owner_id === user?.id;
                    return (
                      <tr key={t.id}>
                        <td>
                          <strong>{t.title}</strong>
                          <div className="small muted">
                            {roleLabel[t.role]} · échéance {fmtDate(t.due_date)}
                          </div>
                          {t.blocked_reason && t.status === "bloque" && <div className="small">En attente : {t.blocked_reason}</div>}
                          {t.done_note && <div className="small">Observation : {t.done_note}</div>}
                        </td>
                        <td>
                          <Badge status={t.status} label={taskStatusLabel[t.status]} />
                          {t.status === "termine" && (
                            <div>
                              {t.resolution ? (
                                <Badge
                                  status={t.resolution === "exception" ? "a_verifier" : "termine"}
                                  label={resolutionLabel[t.resolution]}
                                />
                              ) : (
                                <span className="small muted">Terminée avant le contrôle des preuves</span>
                              )}
                              {t.resolution_evidence?.movements && (
                                <div className="small">Preuve : {t.resolution_evidence.movements.join(", ")}</div>
                              )}
                              {t.resolution_evidence?.remises_d_origine && (
                                <div className="small">Détenu via : {t.resolution_evidence.remises_d_origine.join(", ")}</div>
                              )}
                            </div>
                          )}
                          {t.owner && <div className="small">{t.owner}</div>}
                          {t.completed_at && <div className="small muted">{fmtDateTime(t.completed_at)}</div>}
                        </td>
                        <td className="nowrap">
                          {inTeam && t.status === "a_faire" && (
                            <button className="btn btn-primary btn-small" onClick={() => act(`/tasks/${t.id}/claim`)}>
                              Je prends en charge
                            </button>
                          )}
                          {mine && t.status === "en_cours" && (
                            <div className="row">
                              <TaskShortcut task={t} c={c} />
                              {t.need_category !== null && (
                                <button className="btn btn-small" onClick={() => setDialog({ kind: "complete", task: t })}>
                                  Terminer
                                </button>
                              )}
                              <button className="btn btn-small" onClick={() => setDialog({ kind: "block", task: t })}>
                                Mettre en attente
                              </button>
                              <button className="btn btn-small" onClick={() => act(`/tasks/${t.id}/release`)}>
                                Libérer
                              </button>
                            </div>
                          )}
                          {mine && t.status === "bloque" && (
                            <button className="btn btn-small" onClick={() => act(`/tasks/${t.id}/unblock`)}>
                              Reprendre
                            </button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </Section>

          {scope === "complet" && (
            <Section title="Avis aux équipes">
              {mail_mode === "journal" && (
                <p className="small muted">Mode test : les avis sont consignés, aucun e-mail n'est envoyé.</p>
              )}
              {notifications.length === 0 ? (
                <Empty>Aucun avis.</Empty>
              ) : (
                <ul className="timeline">
                  {notifications.map((n, i) => (
                    <li key={i}>
                      {n.subject}
                      <div className="small">{visibilityText(n.target_roles, mail_mode)}</div>
                      <div className="small muted">
                        {fmtDateTime(n.created_at)} ·{" "}
                        {n.status === "en_attente"
                          ? "destinataires pas encore résolus"
                          : [
                              `${n.recipients} destinataire(s)`,
                              n.logged ? `${n.logged} ${deliveryStatusLabel.journalise.toLowerCase()}` : null,
                              n.accepted ? `${n.accepted} accepté(s) par Exchange` : null,
                              n.pending ? `${n.pending} en attente` : null,
                              n.failed ? `${n.failed} en erreur` : null,
                            ]
                              .filter(Boolean)
                              .join(" · ")}
                      </div>
                      {n.missing_roles?.length > 0 && (
                        <div className="small" style={{ color: "var(--danger)" }}>
                          Aucun utilisateur actif : {n.missing_roles.map((r: string) => roleLabel[r]).join(", ")}
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </Section>
          )}

          {children.length > 0 && (
            <Section title="Demandes liées">
              <ul>
                {children.map((ch) => (
                  <li key={ch.reference}>
                    <Link to={`/dossiers/${ch.reference}`}>{ch.reference}</Link> — {caseKindLabel[ch.kind]} ({caseStatusLabel[ch.status]})
                  </li>
                ))}
              </ul>
            </Section>
          )}

          <Section title="Historique">
            <p className="small muted">
              Salarié : {employeeStatusLabel[c.employee_status]} · créé par {c.created_by_name} le {fmtDateTime(c.created_at)}
            </p>
            <ul className="timeline">
              {history.map((h, i) => (
                <li key={i}>
                  <span className="muted">{fmtDateTime(h.at)}</span> · {h.action.replace(/_/g, " ")} · {h.actor ?? "système"}
                </li>
              ))}
            </ul>
          </Section>
        </div>
      </div>

      {dialog?.kind === "block" && (
        <ReasonDialog
          title="Mettre en attente"
          label="Raison de l'attente (visible des équipes)"
          confirm="Mettre en attente"
          onCancel={() => setDialog(null)}
          onConfirm={async (reason) => {
            await act(`/tasks/${dialog.task.id}/block`, { reason });
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === "complete" && (
        <CompleteTaskDialog
          task={dialog.task}
          employeeId={c.employee_id}
          onClose={() => setDialog(null)}
          onDone={() => {
            setDialog(null);
            detail.reload();
          }}
        />
      )}
      {dialog?.kind === "arrival" && (
        <ConfirmArrivalDialog
          employee={{ id: c.employee_id, first_name: c.first_name, last_name: c.last_name, status: c.employee_status, planned_arrival: c.effective_date }}
          onClose={() => setDialog(null)}
          onDone={() => {
            setDialog(null);
            detail.reload();
          }}
        />
      )}
      {dialog?.kind === "needs" && (
        <EditNeedsDialog detail={detail.data} onClose={() => setDialog(null)} onSaved={() => { setDialog(null); detail.reload(); }} />
      )}
      {dialog?.kind === "close" && (
        <CloseDialog c={c} isAdmin={has("ADMIN")} onClose={() => setDialog(null)} onDone={() => { setDialog(null); detail.reload(); }} />
      )}
    </>
  );
}

/** Raccourci vers l'écran qui réalise concrètement la tâche. */
function TaskShortcut({ task, c }: { task: any; c: any }) {
  const q = new URLSearchParams({ salarie: c.employee_id, dossier: c.id, tache: task.id });
  if (["pc", "telephone", "sim"].includes(task.need_category)) {
    return (
      <Link className="btn btn-primary btn-small" to={`/parc/remise?${q}`}>
        Remettre le matériel
      </Link>
    );
  }
  if (["email_pro", "xena", "intelix"].includes(task.need_category)) {
    return (
      <Link className="btn btn-primary btn-small" to={`/personnel/${c.employee_id}#comptes`}>
        Consigner le compte
      </Link>
    );
  }
  if (task.need_category === null) {
    return (
      <Link className="btn btn-primary btn-small" to="/formations">
        Gérer la session
      </Link>
    );
  }
  return null;
}

function EditNeedsDialog({ detail, onClose, onSaved }: { detail: CaseDetail; onClose: () => void; onSaved: () => void }) {
  const initial = emptyNeeds();
  for (const n of detail.needs) {
    initial[n.category] = { category: n.category, answer: n.answer, preference: n.preference, quantity: n.quantity, detail: n.detail } as NeedValue;
  }
  const [needs, setNeeds] = useState(initial);
  const [trainings, setTrainings] = useState<TrainingValue[]>([]);
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);
  const existing = detail.trainings.map((t: any) => t.catalog_id).filter(Boolean);
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      // Seuls les besoins modifiés sont envoyés.
      const changed = needsPayload(needs).filter((n) => {
        const before = initial[n.category];
        return JSON.stringify(needsPayload({ x: before })[0]) !== JSON.stringify(n);
      });
      await api(`/cases/${detail.case.id}/needs`, { body: { version: detail.case.version, needs: changed, trainings } });
      onSaved();
    } catch (e) {
      setError(e as ApiError);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title={`Compléter ${detail.case.reference}`} onClose={onClose} wide>
      <div>
        <p className="small muted">Seules les nouvelles actions sont créées et seules les équipes concernées sont prévenues.</p>
        <NeedsEditor needs={needs} onChange={setNeeds} trainings={trainings} onTrainings={setTrainings} existingTrainingIds={existing} />
        <ErrorBox error={error} />
        <div className="form-actions">
          <button className="btn" onClick={onClose}>
            Annuler
          </button>
          <button className="btn btn-primary" onClick={save} disabled={busy}>
            Enregistrer et prévenir
          </button>
        </div>
      </div>
    </Modal>
  );
}

function CloseDialog({ c, isAdmin, onClose, onDone }: { c: any; isAdmin: boolean; onClose: () => void; onDone: () => void }) {
  const [note, setNote] = useState("");
  const [exception, setException] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const unresolved: string[] | undefined = error?.detail?.unresolved;
  const submit = async () => {
    setError(null);
    try {
      await api(`/cases/${c.id}/close`, {
        body: { version: c.version, note: note || null, exception_reason: exception || null },
      });
      onDone();
    } catch (e) {
      setError(e as ApiError);
    }
  };
  return (
    <Modal title={`Clôturer ${c.reference}`} onClose={onClose}>
      <p className="small">Les actions non soldées sont listées ; aucune case « tout a été fait ».</p>
      <Field label="Note de clôture">
        <textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} />
      </Field>
      <ErrorBox error={error} />
      {unresolved && isAdmin && (
        <Field label="Exception motivée (ADMIN)" hint="Clôture malgré les points ci-dessus, tracée au journal.">
          <textarea rows={2} value={exception} onChange={(e) => setException(e.target.value)} />
        </Field>
      )}
      <div className="form-actions">
        <button className="btn" onClick={onClose}>
          Annuler
        </button>
        <button className="btn btn-primary" onClick={submit}>
          {exception ? "Clôturer avec exception" : "Clôturer"}
        </button>
      </div>
    </Modal>
  );
}
