import { ReactNode, useEffect, useRef, useState } from "react";
import { ApiError } from "./api";

export function PageHeader({ title, subtitle, actions }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="page-header">
      <div>
        <h1>{title}</h1>
        {subtitle && <p className="subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </div>
  );
}

const tone: Record<string, string> = {
  a_faire: "warn",
  en_cours: "info",
  bloque: "danger",
  termine: "ok",
  annule: "muted",
  ouvert: "info",
  clos: "ok",
  annule_case: "muted",
  a_venir: "info",
  actif: "ok",
  sorti: "muted",
  disponible: "ok",
  reserve: "info",
  affecte: "info",
  controle: "warn",
  reparation: "warn",
  reforme: "muted",
  a_planifier: "warn",
  planifiee: "info",
  reportee: "warn",
  realisee: "ok",
  dispensee: "ok",
  annulee: "muted",
  a_generer: "warn",
  genere: "warn",
  signe: "ok",
  journalise: "muted",
  accepte: "ok",
  a_verifier: "danger",
  echec: "danger",
  en_attente: "warn",
  traite: "ok",
  erreur: "danger",
  oui: "ok",
  non: "muted",
  a_preciser: "warn",
};

/** Pastille d'état : toujours un texte, la couleur n'est qu'un renfort. */
export function Badge({ status, label }: { status: string; label?: string }) {
  return <span className={`badge badge-${tone[status] ?? "muted"}`}>{label ?? status}</span>;
}

export function ErrorBox({ error, onRetry }: { error: ApiError | null | undefined; onRetry?: () => void }) {
  if (!error) return null;
  const unresolved: string[] | undefined = error.detail?.unresolved;
  return (
    <div className="alert alert-danger" role="alert">
      <strong>{error.message}</strong>
      {unresolved && (
        <ul>
          {unresolved.map((u) => (
            <li key={u}>{u}</li>
          ))}
        </ul>
      )}
      {onRetry && (
        <button type="button" className="btn btn-small" onClick={onRetry}>
          Réessayer
        </button>
      )}
    </div>
  );
}

export function Notice({ kind = "info", children }: { kind?: "info" | "ok" | "warn" | "danger"; children: ReactNode }) {
  return (
    <div className={`alert alert-${kind}`} role={kind === "danger" ? "alert" : "status"}>
      {children}
    </div>
  );
}

export function Loading({ what = "Chargement" }: { what?: string }) {
  return (
    <p className="loading" aria-live="polite">
      {what}…
    </p>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="empty">{children}</p>;
}

export function Field({
  label,
  hint,
  error,
  required,
  children,
}: {
  label: string;
  hint?: string;
  error?: string | false | null;
  required?: boolean;
  children: ReactNode;
}) {
  return (
    <label className={`field ${error ? "field-error" : ""}`}>
      <span className="field-label">
        {label}
        {required && <span className="req" aria-hidden="true"> *</span>}
      </span>
      {children}
      {hint && !error && <span className="field-hint">{hint}</span>}
      {error && <span className="field-msg">{error}</span>}
    </label>
  );
}

export function Section({ title, actions, children }: { title: ReactNode; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="card">
      <div className="card-head">
        <h2>{title}</h2>
        {actions && <div className="card-actions">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

/** Fenêtre modale accessible (focus initial, Échap pour fermer). */
export function Modal({ title, onClose, children, wide }: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current?.querySelector<HTMLElement>("input, select, textarea, button");
    el?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`modal ${wide ? "modal-wide" : ""}`} role="dialog" aria-modal="true" aria-label={title} ref={ref}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button type="button" className="btn-icon" onClick={onClose} aria-label="Fermer">
            ×
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

/** Demande un motif avant d'exécuter une action. */
export function ReasonDialog({
  title,
  label,
  confirm,
  onCancel,
  onConfirm,
  optional,
}: {
  title: string;
  label: string;
  confirm: string;
  optional?: boolean;
  onCancel: () => void;
  onConfirm: (reason: string) => Promise<void> | void;
}) {
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <Modal title={title} onClose={onCancel}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (!optional && !reason.trim()) return;
          setBusy(true);
          try {
            await onConfirm(reason.trim());
          } finally {
            setBusy(false);
          }
        }}
      >
        <Field label={label} required={!optional}>
          <textarea value={reason} onChange={(e) => setReason(e.target.value)} rows={3} />
        </Field>
        <div className="form-actions">
          <button type="button" className="btn" onClick={onCancel}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={busy || (!optional && !reason.trim())}>
            {confirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function Pager({ page, hasNext, onPage }: { page: number; hasNext: boolean; onPage: (p: number) => void }) {
  if (page === 1 && !hasNext) return null;
  return (
    <div className="pager">
      <button className="btn btn-small" disabled={page <= 1} onClick={() => onPage(page - 1)}>
        ← Précédent
      </button>
      <span>Page {page}</span>
      <button className="btn btn-small" disabled={!hasNext} onClick={() => onPage(page + 1)}>
        Suivant →
      </button>
    </div>
  );
}
