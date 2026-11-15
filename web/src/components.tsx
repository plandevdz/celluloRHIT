import { ReactNode, useEffect, useRef, useState } from "react";
import { api, ApiError } from "./api";

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
  en_depart: "warn",
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
  en_service: "info",
  immobilise: "warn",
  rendu_loueur: "muted",
  brouillon: "info",
  confirmee: "info",
  appliquee: "ok",
  suspendue: "danger",
  planifie: "info",
  sortie_effective: "ok",
  a_traiter: "warn",
  fait: "ok",
  ecart: "danger",
  perte: "muted",
  non_applicable: "muted",
  vu: "ok",
  ailleurs: "warn",
  hors_perimetre: "warn",
  inconnu: "danger",
  doublon: "muted",
  archive: "ok",
  desactive: "muted",
  creation: "info",
  mise_a_jour: "info",
  inchange: "muted",
  a_revoir: "warn",
  rejet: "danger",
  cree: "ok",
  mis_a_jour: "ok",
  ignore: "muted",
  conflit: "danger",
  rejete: "danger",
  ouverte: "info",
  close: "ok",
  renouvele: "muted",
  restitue: "muted",
  active: "ok",
  resiliee: "muted",
  conserver: "ok",
  ajouter: "info",
  reprendre: "warn",
  verifier: "warn",
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

/** Recherche d'un salarié par nom ou matricule (RH, ADMIN, IT). */
export function EmployeePicker({
  value,
  onChange,
  label = "Salarié",
  required,
  excludeOut = true,
}: {
  value: { id: string; name: string } | null;
  onChange: (v: { id: string; name: string } | null) => void;
  label?: string;
  required?: boolean;
  excludeOut?: boolean;
}) {
  const [q, setQ] = useState("");
  const [items, setItems] = useState<any[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const term = q.trim();
    if (term.length < 2) {
      setItems([]);
      return;
    }
    const t = window.setTimeout(async () => {
      try {
        const r = await api<{ items: any[] }>(`/employees?q=${encodeURIComponent(term)}`);
        setItems(r.items.filter((e) => !excludeOut || e.status !== "sorti").slice(0, 8));
        setError(null);
      } catch (e) {
        setError((e as ApiError).message);
      }
    }, 250);
    return () => window.clearTimeout(t);
  }, [q, excludeOut]);
  if (value) {
    return (
      <div className="field">
        <span className="field-label">{label}</span>
        <div className="row">
          <strong dir="auto">{value.name}</strong>
          <button type="button" className="btn btn-small" onClick={() => onChange(null)}>
            Changer
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className="field">
      <span className="field-label">
        {label}
        {required && <span className="req" aria-hidden="true"> *</span>}
      </span>
      <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Nom ou matricule (2 caractères au moins)" aria-label={label} dir="auto" />
      {error && <span className="field-msg">{error}</span>}
      {items.length > 0 && (
        <ul className="picker-list">
          {items.map((e) => (
            <li key={e.id}>
              <button type="button" className="btn-link" onClick={() => onChange({ id: e.id, name: `${e.first_name} ${e.last_name}` })} dir="auto">
                {e.first_name} {e.last_name}
              </button>{" "}
              <span className="small muted">
                {e.matricule ?? "sans matricule"} · {e.job_title ?? "—"} · {e.site_name ?? "—"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Onglets dont l'état est conservé dans l'URL (?onglet=). */
export function Tabs({ tabs, current, onSelect }: { tabs: { id: string; label: string }[]; current: string; onSelect: (id: string) => void }) {
  return (
    <div className="row" role="tablist" style={{ marginBottom: "1rem" }}>
      {tabs.map((t) => (
        <button key={t.id} role="tab" aria-selected={current === t.id} className={`btn ${current === t.id ? "btn-primary" : ""}`} onClick={() => onSelect(t.id)}>
          {t.label}
        </button>
      ))}
    </div>
  );
}
