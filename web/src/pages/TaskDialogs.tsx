import { FormEvent, useState } from "react";
import { api, ApiError } from "../api";
import { ErrorBox, Field, Modal, Notice } from "../components";
import { useLoad } from "../hooks";
import { fmtDate, fmtDateTime, needLabel, resolutionLabel, todayIso } from "../labels";

const EQUIPMENT = ["pc", "telephone", "sim"];
const ACCOUNTS = ["email_pro", "xena", "intelix"];

function holdingMatches(category: string, h: any): boolean {
  if (category === "pc") return h.item_type === "asset" && ["pc_portable", "pc_fixe"].includes(h.asset_kind);
  if (category === "telephone") return h.item_type === "asset" && h.asset_kind === "telephone";
  if (category === "sim") return h.item_type === "sim";
  return false;
}

/** Fin de tâche avec sa preuve : remis / déjà détenu / exception motivée. */
export function CompleteTaskDialog({
  task,
  employeeId,
  onClose,
  onDone,
}: {
  task: any;
  employeeId: string;
  onClose: () => void;
  onDone: () => void;
}) {
  const category: string = task.need_category;
  const vehicle = category === "vehicule";
  const equipment = EQUIPMENT.includes(category) || vehicle;
  const account = ACCOUNTS.includes(category);
  const delta = !category && !!task.transfer_delta_id;
  const [mode, setMode] = useState<string>(equipment ? "remis" : "realise");
  const [selected, setSelected] = useState<string[]>([]);
  const [note, setNote] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const [busy, setBusy] = useState(false);
  const holdings = useLoad(
    () =>
      vehicle
        ? api<any[]>(`/employees/${employeeId}/vehicles`).then((vs) =>
            vs.map((v) => ({ assignment_id: v.assignment_id, label: `Véhicule ${v.plate}`, identifier: v.label, origin_reference: v.handover_reference, since: v.since, vehicle: true })),
          )
        : equipment
          ? api<any[]>(`/employees/${employeeId}/holdings`)
          : Promise.resolve([]),
    [employeeId, equipment, vehicle],
  );
  const candidates = (holdings.data ?? []).filter((h) => h.vehicle || holdingMatches(category, h));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api(`/tasks/${task.id}/complete`, {
        body: { resolution: mode, assignment_ids: mode === "deja_detenu" ? selected : [], note: note.trim() || null },
      });
      onDone();
    } catch (err) {
      setError(err as ApiError);
    } finally {
      setBusy(false);
    }
  };

  const options: { id: string; label: string; hint: string }[] = delta
    ? [
        {
          id: "realise",
          label: task.title.startsWith("Vérifier") ? "Vérifié" : "Repris",
          hint: task.title.startsWith("Vérifier")
            ? "Indiquez l'état réel constaté (conservé, à reprendre…)."
            : "Vérifié sur la restitution, la coupure ou la libération enregistrée.",
        },
        { id: "exception", label: "Exception motivée", hint: "Soldée sans reprise, sur motif." },
      ]
    : equipment
    ? [
        { id: "remis", label: "Remis", hint: `Vérifié sur les remises enregistrées dans ce dossier (${needLabel[category]}).` },
        { id: "deja_detenu", label: "Déjà détenu", hint: "Le salarié détient déjà ce bien (affectation active vérifiée)." },
        { id: "exception", label: "Exception motivée", hint: "Soldée sans matériel : n'apparaîtra ni comme remise ni sur une décharge." },
      ]
    : account
      ? [
          { id: "realise", label: "Compte consigné", hint: "Vérifié sur les comptes consignés dans la fiche salarié." },
          { id: "exception", label: "Exception motivée", hint: "Soldée sans compte consigné, sur motif." },
        ]
      : [
          { id: "realise", label: "Réalisé", hint: "Travail effectué." },
          { id: "exception", label: "Exception motivée", hint: "Soldée sans réalisation, sur motif." },
        ];

  return (
    <Modal title={`Terminer : ${task.title}`} onClose={onClose}>
      <form onSubmit={submit}>
        <fieldset>
          <legend>Comment la tâche est-elle soldée ?</legend>
          {options.map((o) => (
            <label key={o.id} className="choice-row">
              <input type="radio" name="resolution" checked={mode === o.id} onChange={() => setMode(o.id)} />
              <span>
                <strong>{o.label}</strong> <span className="small muted">— {o.hint}</span>
              </span>
            </label>
          ))}
        </fieldset>
        {mode === "deja_detenu" &&
          (candidates.length === 0 ? (
            <Notice kind="warn">Aucun {needLabel[category]} actuellement détenu par ce salarié.</Notice>
          ) : (
            <fieldset>
              <legend>Bien détenu</legend>
              {candidates.map((h) => (
                <label key={h.assignment_id} className="choice-row">
                  <input
                    type="checkbox"
                    checked={selected.includes(h.assignment_id)}
                    onChange={(e) =>
                      setSelected(e.target.checked ? [...selected, h.assignment_id] : selected.filter((x) => x !== h.assignment_id))
                    }
                  />
                  {h.label} <span className="mono small">{h.identifier}</span>
                  <span className="small muted">
                    ({h.origin_reference}, {fmtDateTime(h.since)})
                  </span>
                </label>
              ))}
            </fieldset>
          ))}
        <Field
          label={mode === "exception" ? "Motif de l'exception" : delta && task.title.startsWith("Vérifier") ? "État réel constaté" : "Observation"}
          required={mode === "exception" || (delta && task.title.startsWith("Vérifier"))}
        >
          <textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} required={mode === "exception" || (delta && task.title.startsWith("Vérifier"))} />
        </Field>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button className="btn btn-primary" type="submit" disabled={busy || (mode === "deja_detenu" && selected.length === 0)}>
            Terminer — {resolutionLabel[mode] ?? "Compte consigné"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** Présence réelle constatée par le RH ; correction de date motivée. */
export function ConfirmArrivalDialog({
  employee,
  onClose,
  onDone,
}: {
  employee: { id: string; first_name: string; last_name: string; status: string; actual_arrival?: string | null; planned_arrival?: string | null };
  onClose: () => void;
  onDone: () => void;
}) {
  const correcting = employee.status === "actif" && !!employee.actual_arrival;
  const [date, setDate] = useState(correcting ? employee.actual_arrival! : todayIso());
  const [reason, setReason] = useState("");
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await api(`/employees/${employee.id}/arrival`, { body: { actual_arrival: date, correction_reason: reason.trim() || null } });
      onDone();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title={`${correcting ? "Corriger la date d'arrivée" : "Confirmer l'arrivée"} — ${employee.first_name} ${employee.last_name}`} onClose={onClose}>
      <form onSubmit={submit}>
        <p className="small">
          {correcting
            ? `Arrivée confirmée le ${fmtDate(employee.actual_arrival)}. Toute correction est motivée et tracée.`
            : `Arrivée prévue le ${fmtDate(employee.planned_arrival)}. La confirmation compte le salarié comme actif ; elle ne clôture pas le dossier (matériel ou formations peuvent suivre).`}
        </p>
        <Field label="Date d'arrivée effective" required>
          <input type="date" value={date} max={todayIso()} onChange={(e) => setDate(e.target.value)} required />
        </Field>
        {correcting && (
          <Field label="Motif de la correction" required>
            <input value={reason} onChange={(e) => setReason(e.target.value)} required />
          </Field>
        )}
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            {correcting ? "Corriger la date" : "Confirmer la présence"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
