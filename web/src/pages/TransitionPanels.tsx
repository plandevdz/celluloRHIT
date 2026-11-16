import { FormEvent, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { useAuth } from "../auth";
import { Badge, EmployeePicker, ErrorBox, Field, Modal, Notice, ReasonDialog, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import {
  deltaActionLabel,
  deltaKindLabel,
  departureReasonLabel,
  departureStatusLabel,
  DEPARTMENTS,
  exitStateLabel,
  fmtDate,
  fmtDateTime,
  fromLocalInput,
  taskStatusLabel,
  todayIso,
  toLocalInput,
  transferStatusLabel,
} from "../labels";

// ------------------------------------------------------------------ mutation

/** M05 : cible saisie, la fiche effective n'est pas modifiée avant l'effet. */
export function TransferDialog({ employee, onClose }: { employee: any; onClose: () => void }) {
  const navigate = useNavigate();
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const profiles = useLoad(() => api<any[]>("/job-profiles"), []);
  const wilayas = useLoad(() => api<any[]>("/wilayas"), []);
  const current: string[] = (employee.wilayas ?? []).map((w: any) => w.id);
  const [changeWilayas, setChangeWilayas] = useState(false);
  const [targetWilayas, setTargetWilayas] = useState<string[]>(current);
  const [f, setF] = useState({
    job_title: employee.job_title ?? "",
    site_id: employee.site_id ?? "",
    department: employee.department ?? "",
    job_profile_id: employee.job_profile_id ?? "",
    effective_date: "",
    note: "",
  });
  const [manager, setManager] = useState<{ id: string; name: string } | null>(
    employee.manager_employee_id ? { id: employee.manager_employee_id, name: employee.manager_employee_name ?? employee.manager } : null,
  );
  const [key] = useState(newKey);
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const body: any = { employee_id: employee.id, effective_date: f.effective_date, note: f.note || null };
      if (f.job_title !== (employee.job_title ?? "")) body.job_title = f.job_title;
      if (f.site_id !== (employee.site_id ?? "")) body.site_id = f.site_id;
      if (f.department !== (employee.department ?? "")) body.department = f.department;
      if (f.job_profile_id && f.job_profile_id !== (employee.job_profile_id ?? "")) body.job_profile_id = f.job_profile_id;
      if (manager && manager.id !== employee.manager_employee_id) body.manager_employee_id = manager.id;
      if (changeWilayas) body.wilaya_ids = targetWilayas;
      const r = await api<{ reference: string }>("/transfers", { idempotencyKey: key, body });
      navigate(`/dossiers/${r.reference}`);
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={`Mutation — ${employee.first_name} ${employee.last_name}`} onClose={onClose} wide>
      <form onSubmit={submit}>
        <p className="small muted">
          Les valeurs actuelles sont conservées sauf modification. Rien n'est appliqué avant la date d'effet ; la comparaison des accès et des
          biens vient ensuite, dans le dossier.
        </p>
        <div className="form-grid">
          <Field label="Poste cible" required>
            <input value={f.job_title} onChange={(e) => setF({ ...f, job_title: e.target.value })} required />
          </Field>
          <Field label="Site cible" required>
            <select value={f.site_id} onChange={(e) => setF({ ...f, site_id: e.target.value })} required>
              {sites.data?.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Département">
            <input list="departements" value={f.department} onChange={(e) => setF({ ...f, department: e.target.value })} />
            <datalist id="departements">
              {DEPARTMENTS.map((d) => (
                <option key={d} value={d} />
              ))}
            </datalist>
          </Field>
          <Field label="Profil de poste cible" hint="Sert à proposer quoi conserver, ajouter, reprendre ou vérifier.">
            <select value={f.job_profile_id} onChange={(e) => setF({ ...f, job_profile_id: e.target.value })}>
              <option value="">Aucun (tout sera « à vérifier »)</option>
              {profiles.data?.filter((p) => p.active).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Date d'effet" required>
            <input type="date" value={f.effective_date} onChange={(e) => setF({ ...f, effective_date: e.target.value })} required />
          </Field>
        </div>
        <EmployeePicker value={manager} onChange={setManager} label="Manager cible (facultatif)" />
        <fieldset>
          <legend>Wilayas couvertes après la mutation</legend>
          <label className="inline-choice">
            <input type="checkbox" checked={changeWilayas} onChange={(e) => setChangeWilayas(e.target.checked)} /> Modifier les wilayas (sinon elles restent
            inchangées : {(employee.wilayas ?? []).map((w: any) => `${w.code} ${w.name}`).join(", ") || "aucune"})
          </label>
          {changeWilayas && (
            <div className="inline-choice" style={{ maxHeight: 180, overflowY: "auto" }}>
              {wilayas.data?.filter((w) => w.active).map((w) => (
                <label key={w.id}>
                  <input
                    type="checkbox"
                    checked={targetWilayas.includes(w.id)}
                    onChange={(e) => setTargetWilayas(e.target.checked ? [...targetWilayas, w.id] : targetWilayas.filter((x) => x !== w.id))}
                  />
                  {w.code} {w.name}
                </label>
              ))}
            </div>
          )}
        </fieldset>
        <Field label="Note RH">
          <textarea rows={2} value={f.note} onChange={(e) => setF({ ...f, note: e.target.value })} />
        </Field>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            Préparer la mutation
          </button>
        </div>
      </form>
    </Modal>
  );
}

const DECISIONS: Record<string, string[]> = {
  besoin: ["ajouter", "conserver"],
  formation: ["ajouter", "conserver"],
};
const HELD = ["conserver", "reprendre", "verifier"];

export function TransferPanel({ t, onChange }: { t: any; onChange: () => void }) {
  const [decisions, setDecisions] = useState<Record<string, string>>(() => Object.fromEntries(t.deltas.map((d: any) => [d.id, d.decision])));
  const [dialog, setDialog] = useState<null | "report" | "annuler">(null);
  const [date, setDate] = useState(t.effective_date);
  const cmd = useCommand();
  const editable = ["brouillon", "suspendue"].includes(t.status);
  const dirty = t.deltas.some((d: any) => decisions[d.id] !== d.decision);
  const save = async () => {
    const changed = t.deltas.filter((d: any) => decisions[d.id] !== d.decision).map((d: any) => ({ id: d.id, decision: decisions[d.id] }));
    const r = await cmd.run(() => api(`/transfers/${t.id}/deltas`, { body: { version: t.version, deltas: changed } }));
    if (r) onChange();
    return r;
  };
  const confirm = async () => {
    let version = t.version;
    if (dirty) {
      const r: any = await save();
      if (!r) return;
      version = r.version;
    }
    const r = await cmd.run(() => api(`/transfers/${t.id}/confirm`, { body: { version } }));
    if (r) onChange();
  };
  const snap = t.snapshot.position;
  return (
    <Section
      title="Mutation"
      actions={<Badge status={t.status} label={transferStatusLabel[t.status]} />}
    >
      {t.status === "suspendue" && (
        <Notice kind="danger">
          Application suspendue : {t.conflict?.motif ?? "situation modifiée"}. Recalculez la comparaison avec la situation actuelle, ou annulez.
        </Notice>
      )}
      {t.position_changed && t.status !== "suspendue" && (
        <Notice kind="warn">Le poste a changé depuis la comparaison : recalculez avant de confirmer.</Notice>
      )}
      <table>
        <thead>
          <tr>
            <th></th>
            <th>Actuel</th>
            <th>Cible</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Poste</td>
            <td>{snap.job_title}</td>
            <td>{t.target.job_title}</td>
          </tr>
          <tr>
            <td>Site</td>
            <td>{snap.site}</td>
            <td>{t.target.site}</td>
          </tr>
          <tr>
            <td>Département</td>
            <td>{snap.department ?? "—"}</td>
            <td>{t.target.department ?? "—"}</td>
          </tr>
          <tr>
            <td>Manager</td>
            <td>{snap.manager ?? "—"}</td>
            <td>{t.target.manager ?? "—"}</td>
          </tr>
          <tr>
            <td>Profil</td>
            <td colSpan={2}>{t.target.profile ?? "aucun profil cible"}</td>
          </tr>
          <tr>
            <td>Wilayas</td>
            <td colSpan={2}>{t.target.wilayas == null ? "inchangées" : t.target.wilayas.length ? t.target.wilayas.join(", ") : "plus aucune wilaya"}</td>
          </tr>
          <tr>
            <td>Date d'effet</td>
            <td colSpan={2}>
              <strong>{fmtDate(t.effective_date)}</strong>
              {t.applied_at && ` · appliquée le ${fmtDateTime(t.applied_at)}`}
              {t.confirmed_at && !t.applied_at && ` · confirmée le ${fmtDateTime(t.confirmed_at)}`}
            </td>
          </tr>
        </tbody>
      </table>
      <h3 style={{ marginTop: "1rem" }}>Actuel / Cible / Action proposée</h3>
      {t.deltas.length === 0 ? (
        <p className="muted small">Aucun accès, bien ni besoin à comparer.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Élément</th>
              <th>Actuel</th>
              <th>Cible</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {t.deltas.map((d: any) => (
              <tr key={d.id}>
                <td>
                  <span className="tag">{deltaKindLabel[d.object_kind]}</span> {d.label}
                </td>
                <td className="small">{d.current ?? "—"}</td>
                <td className="small">{d.target ?? "à vérifier (pas de cible)"}</td>
                <td>
                  {editable ? (
                    <select value={decisions[d.id]} onChange={(e) => setDecisions({ ...decisions, [d.id]: e.target.value })} aria-label={`Action pour ${d.label}`}>
                      {(DECISIONS[d.object_kind] ?? HELD).map((a) => (
                        <option key={a} value={a}>
                          {deltaActionLabel[a]}
                          {a === d.proposal ? " (proposé)" : ""}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <>
                      <Badge status={d.decision} label={deltaActionLabel[d.decision]} />
                      {d.task_status && <div className="small muted">tâche : {taskStatusLabel[d.task_status]}</div>}
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <ErrorBox error={cmd.error} />
      <div className="form-actions">
        {editable && dirty && (
          <button className="btn" onClick={save} disabled={cmd.busy}>
            Enregistrer les choix
          </button>
        )}
        {["brouillon", "suspendue", "confirmee"].includes(t.status) && (
          <>
            {(t.status === "suspendue" || t.position_changed) && (
              <button className="btn" disabled={cmd.busy} onClick={async () => { const r = await cmd.run(() => api(`/transfers/${t.id}/refresh`, { body: { version: t.version } })); if (r) onChange(); }}>
                Recalculer la comparaison
              </button>
            )}
            <button className="btn" onClick={() => setDialog("report")}>
              Reporter
            </button>
            <button className="btn btn-danger" onClick={() => setDialog("annuler")}>
              Annuler la mutation
            </button>
          </>
        )}
        {editable && !t.position_changed && (
          <button className="btn btn-primary" onClick={confirm} disabled={cmd.busy}>
            Confirmer la cible et ouvrir les actions
          </button>
        )}
      </div>
      {t.status === "appliquee" && (
        <p className="small muted">
          Poste appliqué. Une erreur se corrige par une correction de poste datée depuis la fiche salarié, jamais en réécrivant le passé.
        </p>
      )}
      {dialog === "report" && (
        <Modal title="Reporter la mutation" onClose={() => setDialog(null)}>
          <ReportForm
            date={date}
            setDate={setDate}
            onCancel={() => setDialog(null)}
            onConfirm={async (reason) => {
              const r = await cmd.run(() => api(`/transfers/${t.id}/reschedule`, { body: { version: t.version, effective_date: date, reason } }));
              if (r) {
                setDialog(null);
                onChange();
              }
            }}
          />
          <ErrorBox error={cmd.error} />
        </Modal>
      )}
      {dialog === "annuler" && (
        <ReasonDialog
          title="Annuler la mutation"
          label="Motif (les actions non faites sont annulées, l'historique reste)"
          confirm="Annuler la mutation"
          onCancel={() => setDialog(null)}
          onConfirm={async (reason) => {
            const r = await cmd.run(() => api(`/transfers/${t.id}/cancel`, { body: { version: t.version, reason } }));
            setDialog(null);
            if (r) onChange();
          }}
        />
      )}
    </Section>
  );
}

function ReportForm({ date, setDate, onCancel, onConfirm }: { date: string; setDate: (d: string) => void; onCancel: () => void; onConfirm: (reason: string) => void }) {
  const [reason, setReason] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onConfirm(reason);
      }}
    >
      <Field label="Nouvelle date d'effet" required>
        <input type="date" value={date} onChange={(e) => setDate(e.target.value)} required />
      </Field>
      <Field label="Motif" required>
        <input value={reason} onChange={(e) => setReason(e.target.value)} required />
      </Field>
      <p className="small muted">Seules les actions encore à faire sont décalées.</p>
      <div className="form-actions">
        <button type="button" className="btn" onClick={onCancel}>
          Annuler
        </button>
        <button type="submit" className="btn btn-primary">
          Reporter
        </button>
      </div>
    </form>
  );
}

/** Correction de poste datée (après une mutation appliquée, ou rectification). */
export function PositionCorrectionDialog({ employee, onClose, onDone }: { employee: any; onClose: () => void; onDone: () => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({ job_title: employee.job_title ?? "", site_id: employee.site_id ?? "", department: employee.department ?? "", valid_from: todayIso(), reason: "" });
  const cmd = useCommand();
  return (
    <Modal title="Correction de poste datée" onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/employees/${employee.id}/positions`, {
              body: { version: employee.version, ...f, department: f.department || null, manager_employee_id: employee.manager_employee_id ?? null, job_profile_id: employee.job_profile_id ?? null },
            }),
          );
          if (r) onDone();
        }}
      >
        <p className="small muted">La situation en cours se termine à la date indiquée ; une nouvelle commence. L'historique n'est pas réécrit.</p>
        <div className="form-grid">
          <Field label="Poste" required>
            <input value={f.job_title} onChange={(e) => setF({ ...f, job_title: e.target.value })} required />
          </Field>
          <Field label="Site" required>
            <select value={f.site_id} onChange={(e) => setF({ ...f, site_id: e.target.value })} required>
              {sites.data?.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Département">
            <input value={f.department} onChange={(e) => setF({ ...f, department: e.target.value })} />
          </Field>
          <Field label="À partir du" required>
            <input type="date" value={f.valid_from} onChange={(e) => setF({ ...f, valid_from: e.target.value })} required />
          </Field>
        </div>
        <Field label="Motif de la correction" required>
          <input value={f.reason} onChange={(e) => setF({ ...f, reason: e.target.value })} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy}>
            Enregistrer la correction
          </button>
        </div>
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------------ départ

/** M06 : le RH décide des dates ; aucun préavis calculé, aucune paie. */
export function DepartureDialog({ employee, onClose }: { employee: any; onClose: () => void }) {
  const navigate = useNavigate();
  const [f, setF] = useState({ reason: "demission", private_note: "", last_day: "", cutoff: "", hr_contact: "", data_transfer: false });
  const [transferAction, setTransferAction] = useState<string | null>(null);
  const [key] = useState(newKey);
  const [error, setError] = useState<ApiError | null>(null);
  const transferRef: string | undefined = error?.detail?.transfer_reference;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const r = await api<{ reference: string }>("/departures", {
        idempotencyKey: transferAction ? `${key}-${transferAction}` : key,
        body: {
          employee_id: employee.id,
          reason: f.reason,
          private_note: f.private_note || null,
          last_day: f.last_day,
          cutoff_at: fromLocalInput(f.cutoff),
          hr_contact: f.hr_contact || null,
          data_transfer: f.data_transfer,
          transfer_action: transferAction,
        },
      });
      navigate(`/dossiers/${r.reference}`);
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={`Départ — ${employee.first_name} ${employee.last_name}`} onClose={onClose} wide>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="Motif opérationnel" required>
            <select value={f.reason} onChange={(e) => setF({ ...f, reason: e.target.value })}>
              {Object.entries(departureReasonLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Dernier jour" required>
            <input type="date" value={f.last_day} onChange={(e) => setF({ ...f, last_day: e.target.value, cutoff: f.cutoff || (e.target.value ? `${e.target.value}T17:00` : "") })} required />
          </Field>
          <Field label="Coupure des accès (heure d'Alger)" required hint="Instant convenu, distinct du dernier jour.">
            <input type="datetime-local" value={f.cutoff} onChange={(e) => setF({ ...f, cutoff: e.target.value })} required />
          </Field>
          <Field label="Contact RH">
            <input value={f.hr_contact} onChange={(e) => setF({ ...f, hr_contact: e.target.value })} />
          </Field>
        </div>
        <Field label="Note privée RH" hint="Visible du RH et de l'ADMIN seulement ; jamais transmise aux équipes.">
          <textarea rows={2} value={f.private_note} onChange={(e) => setF({ ...f, private_note: e.target.value })} />
        </Field>
        <label className="inline-choice" style={{ marginBottom: ".8rem" }}>
          <input type="checkbox" checked={f.data_transfer} onChange={(e) => setF({ ...f, data_transfer: e.target.checked })} /> Transfert de données autorisé
          à organiser par l'IT
        </label>
        <p className="small muted">
          Les comptes, licences, biens (y compris prêts directs et demandes ponctuelles) et véhicules réellement confiés sont relevés
          automatiquement. L'application ne coupe aucun compte : l'IT confirme chaque coupure.
        </p>
        {transferRef ? (
          <Notice kind="warn">
            {error?.message}.{" "}
            <button type="button" className="btn btn-small" onClick={() => setTransferAction("annuler")}>
              Annuler la mutation {transferRef}
            </button>{" "}
            <button type="button" className="btn btn-small" onClick={() => setTransferAction("conserver")}>
              La conserver
            </button>
            {transferAction && <div className="small">Choix : {transferAction === "annuler" ? "annuler la mutation" : "conserver la mutation"} — validez de nouveau.</div>}
          </Notice>
        ) : (
          <ErrorBox error={error} />
        )}
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            Planifier le départ et prévenir les équipes
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function DeparturePanel({ c, d, onChange }: { c: any; d: any; onChange: () => void }) {
  const { has } = useAuth();
  const hr = has("RH", "ADMIN");
  const cmd = useCommand();
  const [dialog, setDialog] = useState<null | { kind: "confirm" | "gap" | "loss" | "na"; control: any } | { kind: "cutoff" | "reschedule" | "cancel" }>(null);
  const access = d.controls.filter((x: any) => x.kind === "compte");
  const goods = d.controls.filter((x: any) => x.kind !== "compte");
  const pending = (x: any) => ["a_traiter", "ecart"].includes(x.state);
  const accessOpen = access.filter(pending).length;
  const goodsOpen = goods.filter(pending).length;
  const planned = d.status === "planifie";
  const accessLabel =
    access.length === 0 ? "Aucun accès consigné" : accessOpen === 0 ? "Accès coupés (confirmés par l'IT)" : d.access_late ? "Accès à vérifier" : d.access_due ? "Coupure exigible" : "Coupure planifiée";
  return (
    <Section title="Départ" actions={<Badge status={d.status} label={departureStatusLabel[d.status]} />}>
      <div className="state-strip">
        <Badge status={c.employee_status === "sorti" ? "sortie_effective" : "planifie"} label={c.employee_status === "sorti" ? "Salarié sorti" : "Salarié encore présent"} />
        <Badge status={accessOpen === 0 ? "fait" : d.access_late ? "echec" : "a_traiter"} label={accessLabel} />
        <Badge status={goodsOpen === 0 ? "fait" : "a_traiter"} label={goodsOpen === 0 ? "Biens soldés" : `${goodsOpen} bien(s) à restituer ou arbitrer`} />
      </div>
      <dl className="kv">
        <dt>Dernier jour</dt>
        <dd>
          <strong>{fmtDate(d.last_day)}</strong>
        </dd>
        <dt>Coupure prévue</dt>
        <dd>
          <strong>{fmtDateTime(d.cutoff_at)}</strong> (heure d'Alger)
          {d.cutoff_notified_at && <div className="small muted">IT alertée le {fmtDateTime(d.cutoff_notified_at)}</div>}
          {d.escalated_at && <div className="small" style={{ color: "var(--danger)" }}>Relance « accès à vérifier » le {fmtDateTime(d.escalated_at)}</div>}
        </dd>
        {hr && (
          <>
            <dt>Motif</dt>
            <dd>{departureReasonLabel[d.reason]}</dd>
            {d.private_note && (
              <>
                <dt>Note privée</dt>
                <dd>{d.private_note}</dd>
              </>
            )}
            {d.hr_contact && (
              <>
                <dt>Contact RH</dt>
                <dd>{d.hr_contact}</dd>
              </>
            )}
          </>
        )}
        {d.exit_applied_at && (
          <>
            <dt>Sortie administrative</dt>
            <dd>enregistrée le {fmtDateTime(d.exit_applied_at)}</dd>
          </>
        )}
      </dl>
      {hr && planned && (
        <div className="row" style={{ margin: ".6rem 0" }}>
          <button className="btn btn-small" onClick={() => setDialog({ kind: "cutoff" })}>
            Avancer ou décaler la coupure
          </button>
          <button className="btn btn-small" onClick={() => setDialog({ kind: "reschedule" })}>
            Reporter le départ
          </button>
          <button className="btn btn-small btn-danger" onClick={() => setDialog({ kind: "cancel" })}>
            Annuler le départ
          </button>
        </div>
      )}
      <ErrorBox error={cmd.error} />
      <div className="panel-pair">
        <ControlList title="Accès à traiter" items={access} hr={hr} onAction={(kind, control) => setDialog({ kind, control })} employeeId={c.employee_id} />
        <ControlList title="Biens à restituer" items={goods} hr={hr} onAction={(kind, control) => setDialog({ kind, control })} employeeId={c.employee_id} />
      </div>
      {dialog && "control" in dialog && dialog.kind === "confirm" && (
        <ConfirmControlDialog control={dialog.control} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChange(); }} />
      )}
      {dialog && "control" in dialog && dialog.kind === "gap" && (
        <GapDialog control={dialog.control} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChange(); }} />
      )}
      {dialog && "control" in dialog && (dialog.kind === "loss" || dialog.kind === "na") && (
        <ReasonDialog
          title={dialog.kind === "loss" ? `Décider la perte — ${dialog.control.label}` : `Sans objet — ${dialog.control.label}`}
          label={dialog.kind === "loss" ? "Motif de la décision (distincte d'un retour)" : "Motif (accès transféré avec autorisation, conservé par décision…)"}
          confirm="Enregistrer la décision"
          onCancel={() => setDialog(null)}
          onConfirm={async (reason) => {
            const r = await cmd.run(() => api(`/exit-controls/${(dialog as any).control.id}/${dialog.kind === "loss" ? "loss" : "not-applicable"}`, { body: { reason } }));
            setDialog(null);
            if (r) onChange();
          }}
        />
      )}
      {dialog?.kind === "cutoff" && <CutoffDialog d={d} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChange(); }} />}
      {dialog?.kind === "reschedule" && <RescheduleDepartureDialog d={d} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChange(); }} />}
      {dialog?.kind === "cancel" && <CancelDepartureDialog d={d} access={access} onClose={() => setDialog(null)} onDone={() => { setDialog(null); onChange(); }} />}
    </Section>
  );
}

function ControlList({ title, items, hr, onAction, employeeId }: { title: string; items: any[]; hr: boolean; onAction: (k: "confirm" | "gap" | "loss" | "na", c: any) => void; employeeId: string }) {
  const { has } = useAuth();
  return (
    <div className="card">
      <h3>{title}</h3>
      {items.length === 0 ? (
        <p className="muted small">Rien à traiter.</p>
      ) : (
        <table>
          <tbody>
            {items.map((x) => {
              const open = ["a_traiter", "ecart"].includes(x.state);
              const canConfirm = x.kind === "vehicule" ? has("RH", "ADMIN") : has("IT", "ADMIN");
              return (
                <tr key={x.id}>
                  <td>
                    <strong>{x.label}</strong>
                    {x.detail && <div className="small muted">{x.detail}</div>}
                    {x.task && (
                      <div className="small muted">
                        tâche {taskStatusLabel[x.task.status]}
                        {x.task.owner && ` · ${x.task.owner}`}
                      </div>
                    )}
                    {x.done_at && (
                      <div className="small">
                        réalisé le {fmtDateTime(x.done_at)}
                        {x.system && ` · ${x.system}`}
                        {x.confirmed_by && ` · ${x.confirmed_by}`}
                      </div>
                    )}
                    {x.note && <div className="small">{x.note}</div>}
                    {x.follow_up_date && <div className="small">suivi le {fmtDate(x.follow_up_date)}</div>}
                  </td>
                  <td>
                    <Badge status={x.state} label={exitStateLabel[x.state]} />
                  </td>
                  <td>
                    {open && (
                      <div className="row">
                        {canConfirm && x.kind === "compte" && (
                          <button className="btn btn-primary btn-small" onClick={() => onAction("confirm", x)}>
                            Coupure réalisée
                          </button>
                        )}
                        {canConfirm && x.kind === "licence" && (
                          <button className="btn btn-primary btn-small" onClick={() => onAction("confirm", x)}>
                            Siège libéré
                          </button>
                        )}
                        {canConfirm && x.kind === "bien" && (
                          <Link className="btn btn-primary btn-small" to={`/parc/restitution?salarie=${employeeId}`}>
                            Enregistrer le retour
                          </Link>
                        )}
                        {canConfirm && x.kind === "vehicule" && (
                          <Link className="btn btn-primary btn-small" to="/vehicules">
                            Restitution véhicule
                          </Link>
                        )}
                        {canConfirm && ["donnees", "autre"].includes(x.kind) && (
                          <button className="btn btn-primary btn-small" onClick={() => onAction("confirm", x)}>
                            Fait
                          </button>
                        )}
                        <button className="btn btn-small" onClick={() => onAction("gap", x)}>
                          Signaler un écart
                        </button>
                        {hr && ["bien", "vehicule"].includes(x.kind) && (
                          <button className="btn btn-small" onClick={() => onAction("loss", x)}>
                            Perte
                          </button>
                        )}
                        {hr && (
                          <button className="btn btn-small" onClick={() => onAction("na", x)}>
                            Sans objet
                          </button>
                        )}
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}

function ConfirmControlDialog({ control, onClose, onDone }: { control: any; onClose: () => void; onDone: () => void }) {
  const [at, setAt] = useState("");
  const [system, setSystem] = useState(control.detail ?? "");
  const [note, setNote] = useState("");
  const cmd = useCommand();
  return (
    <Modal title={control.kind === "compte" ? `Coupure réalisée — ${control.label}` : control.label} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() => api(`/exit-controls/${control.id}/confirm`, { body: { done_at: fromLocalInput(at), system: system || null, note: note || null } }));
          if (r) onDone();
        }}
      >
        {control.kind === "compte" && (
          <>
            <p className="small">Vous confirmez avoir désactivé ce compte dans le système externe. L'application ne coupe rien elle-même.</p>
            <div className="form-grid">
              <Field label="Heure réelle de la coupure (Alger)" hint="Vide : maintenant">
                <input type="datetime-local" value={at} max={toLocalInput(new Date().toISOString())} onChange={(e) => setAt(e.target.value)} />
              </Field>
              <Field label="Système">
                <input value={system} onChange={(e) => setSystem(e.target.value)} />
              </Field>
            </div>
          </>
        )}
        <Field label="Observation">
          <input value={note} onChange={(e) => setNote(e.target.value)} />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy}>
            Confirmer
          </button>
        </div>
      </form>
    </Modal>
  );
}

function GapDialog({ control, onClose, onDone }: { control: any; onClose: () => void; onDone: () => void }) {
  const [note, setNote] = useState(control.note ?? "");
  const [date, setDate] = useState(control.follow_up_date ?? "");
  const cmd = useCommand();
  return (
    <Modal title={`Signaler un écart — ${control.label}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() => api(`/exit-controls/${control.id}/gap`, { body: { note, follow_up_date: date || null } }));
          if (r) onDone();
        }}
      >
        <Field label="Écart constaté et prochaine action" required>
          <textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} required />
        </Field>
        <Field label="Date de suivi">
          <input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy}>
            Enregistrer
          </button>
        </div>
      </form>
    </Modal>
  );
}

function CutoffDialog({ d, onClose, onDone }: { d: any; onClose: () => void; onDone: () => void }) {
  const [at, setAt] = useState(toLocalInput(d.cutoff_at));
  const [reason, setReason] = useState("");
  const cmd = useCommand();
  return (
    <Modal title="Heure de coupure" onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() => api(`/departures/${d.id}/cutoff`, { body: { version: d.version, cutoff_at: fromLocalInput(at), reason } }));
          if (r) onDone();
        }}
      >
        <Field label="Nouvelle coupure (heure d'Alger)" required>
          <input type="datetime-local" value={at} onChange={(e) => setAt(e.target.value)} required />
        </Field>
        <Field label="Motif (une coupure avancée prévient l'IT immédiatement)" required>
          <input value={reason} onChange={(e) => setReason(e.target.value)} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy}>
            Enregistrer et prévenir l'IT
          </button>
        </div>
      </form>
    </Modal>
  );
}

function RescheduleDepartureDialog({ d, onClose, onDone }: { d: any; onClose: () => void; onDone: () => void }) {
  const [day, setDay] = useState(d.last_day);
  const [at, setAt] = useState(toLocalInput(d.cutoff_at));
  const [reason, setReason] = useState("");
  const cmd = useCommand();
  return (
    <Modal title="Reporter le départ" onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() => api(`/departures/${d.id}/reschedule`, { body: { version: d.version, last_day: day, cutoff_at: fromLocalInput(at), reason } }));
          if (r) onDone();
        }}
      >
        <div className="form-grid">
          <Field label="Dernier jour" required>
            <input type="date" value={day} onChange={(e) => setDay(e.target.value)} required />
          </Field>
          <Field label="Coupure (heure d'Alger)" required>
            <input type="datetime-local" value={at} onChange={(e) => setAt(e.target.value)} required />
          </Field>
        </div>
        <Field label="Motif" required>
          <input value={reason} onChange={(e) => setReason(e.target.value)} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy}>
            Reporter
          </button>
        </div>
      </form>
    </Modal>
  );
}

function CancelDepartureDialog({ d, access, onClose, onDone }: { d: any; access: any[]; onClose: () => void; onDone: () => void }) {
  const [reason, setReason] = useState("");
  const [restore, setRestore] = useState<string[]>([]);
  const cmd = useCommand();
  const cut = access.filter((x) => x.state === "fait");
  return (
    <Modal title="Annuler le départ" onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const ids = cut.filter((x) => restore.includes(x.id)).map((x) => x.account_id ?? x.id);
          const r = await cmd.run(() => api(`/departures/${d.id}/cancel`, { body: { version: d.version, reason, restore_account_ids: ids } }));
          if (r) onDone();
        }}
      >
        <p className="small">Les actions non réalisées sont annulées ; l'historique est conservé.</p>
        {cut.length > 0 && (
          <fieldset>
            <legend>Accès déjà coupés : rétablissement à demander explicitement</legend>
            {cut.map((x) => (
              <label key={x.id} className="choice-row">
                <input type="checkbox" checked={restore.includes(x.id)} onChange={(e) => setRestore(e.target.checked ? [...restore, x.id] : restore.filter((r) => r !== x.id))} />
                {x.label}
              </label>
            ))}
          </fieldset>
        )}
        <Field label="Motif" required>
          <input value={reason} onChange={(e) => setReason(e.target.value)} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Fermer
          </button>
          <button type="submit" className="btn btn-danger" disabled={cmd.busy}>
            Annuler le départ
          </button>
        </div>
      </form>
    </Modal>
  );
}

