import { FormEvent, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { useAuth } from "../auth";
import { ErrorBox, Field, Notice, PageHeader } from "../components";
import { useLoad, useUnsavedWarning } from "../hooks";
import { employeeStatusLabel, fmtDate, roleLabel } from "../labels";
import { emptyNeeds, NeedsEditor, needsPayload, recipients, TrainingValue } from "./NeedsEditor";

type Site = { id: string; name: string; active: boolean };
type Candidate = {
  id: string;
  matricule: string | null;
  first_name: string;
  last_name: string;
  status: string;
  job_title: string | null;
  site_name: string | null;
  same_matricule: boolean;
};

const EMPTY = { first_name: "", last_name: "", job_title: "", site_id: "", planned_arrival: "" };

/** M01 / U1 : cinq champs, un bouton, avis immédiat aux équipes. */
export default function ArrivalPage() {
  const { has } = useAuth();
  const sites = useLoad(() => api<Site[]>("/sites"), []);
  const [form, setForm] = useState(EMPTY);
  const [optional, setOptional] = useState({ matricule: "", department: "", manager: "", note: "" });
  const [needs, setNeeds] = useState(emptyNeeds);
  const [trainings, setTrainings] = useState<TrainingValue[]>([]);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [chosen, setChosen] = useState<string | "new" | null>(null);
  const [key, setKey] = useState(newKey);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [result, setResult] = useState<{ reference: string; notified_roles: string[]; tasks_created: number } | null>(null);

  const dirty = !result && Object.values(form).some((v) => v !== "");
  useUnsavedWarning(dirty);

  // Ressemblances : avertissement non bloquant, la décision reste au RH.
  useEffect(() => {
    if (form.first_name.trim().length < 2 || form.last_name.trim().length < 2) {
      setCandidates([]);
      return;
    }
    const t = window.setTimeout(async () => {
      try {
        const q = new URLSearchParams({ first_name: form.first_name, last_name: form.last_name, matricule: optional.matricule });
        const c = await api<Candidate[]>(`/employees/similar?${q}`);
        setCandidates(c);
        setChosen(null);
      } catch {
        /* sans effet sur la saisie */
      }
    }, 400);
    return () => window.clearTimeout(t);
  }, [form.first_name, form.last_name, optional.matricule]);

  if (!has("RH", "ADMIN")) return <Notice kind="danger">Réservé au RH et à l'administration.</Notice>;

  const set = (k: keyof typeof EMPTY) => (e: { target: { value: string } }) => setForm({ ...form, [k]: e.target.value });
  const fieldError = (f: string) => (error?.field === f ? error.message : null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (candidates.length > 0 && chosen === null) {
      setError(new ApiError(422, "invalide", "Indiquez s'il s'agit d'une personne déjà connue ou d'une nouvelle personne.", "candidates"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const res = await api<{ reference: string; notified_roles: string[]; tasks_created: number }>("/arrivals", {
        idempotencyKey: key,
        body: {
          ...form,
          employee_id: chosen && chosen !== "new" ? chosen : null,
          matricule: optional.matricule || null,
          department: optional.department || null,
          manager: optional.manager || null,
          note: optional.note || null,
          needs: needsPayload(needs),
          trainings,
        },
      });
      setResult(res);
    } catch (err) {
      setError(err as ApiError);
    } finally {
      setBusy(false);
    }
  };

  const reset = () => {
    setForm(EMPTY);
    setOptional({ matricule: "", department: "", manager: "", note: "" });
    setNeeds(emptyNeeds());
    setTrainings([]);
    setCandidates([]);
    setChosen(null);
    setResult(null);
    setError(null);
    setKey(newKey());
  };

  if (result) {
    return (
      <>
        <PageHeader title="Arrivée annoncée" />
        <div className="success-panel">
          <div className="ref">{result.reference} créé</div>
          <p>
            Équipes prévenues : <strong>{result.notified_roles.map((r) => roleLabel[r]).join(", ")}</strong>.{" "}
            {result.tasks_created > 0 ? `${result.tasks_created} action(s) créée(s).` : "Aucune action précise : annonce seule."}
          </p>
          <p className="small">
            Notifications : en file d'envoi (suivi dans le dossier). Le dossier est déjà visible dans « Mon travail » des
            équipes.
          </p>
          <div className="row">
            <Link className="btn btn-primary" to={`/dossiers/${result.reference}`}>
              Ouvrir le dossier
            </Link>
            <button className="btn" onClick={reset}>
              Annoncer une autre arrivée
            </button>
          </div>
        </div>
      </>
    );
  }

  const activeSites = sites.data?.filter((s) => s.active) ?? [];

  return (
    <>
      <PageHeader title="Annoncer une arrivée" subtitle="Cinq informations suffisent. Les besoins peuvent être précisés plus tard." />
      <form onSubmit={submit} className="stack">
        <section className="card">
          <div className="form-grid">
            <Field label="Prénom" required error={fieldError("first_name")}>
              <input value={form.first_name} onChange={set("first_name")} required autoFocus dir="auto" />
            </Field>
            <Field label="Nom" required error={fieldError("last_name")}>
              <input value={form.last_name} onChange={set("last_name")} required dir="auto" />
            </Field>
            <Field label="Poste prévu" required error={fieldError("job_title")}>
              <input value={form.job_title} onChange={set("job_title")} required />
            </Field>
            <Field label="Site" required error={fieldError("site_id")}>
              <select value={form.site_id} onChange={set("site_id")} required>
                <option value="">Choisir…</option>
                {activeSites.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Date d'arrivée prévue" required error={fieldError("planned_arrival")}>
              <input type="date" value={form.planned_arrival} onChange={set("planned_arrival")} required />
            </Field>
          </div>
          {activeSites.length === 0 && !sites.loading && (
            <Notice kind="warn">Aucun site n'est configuré : l'administrateur doit d'abord créer les sites.</Notice>
          )}

          {candidates.length > 0 && (
            <Notice kind="warn">
              <strong>Personne(s) ressemblante(s) déjà connue(s)</strong> — vérifiez avant de créer :
              <div className="stack" style={{ marginTop: ".5rem", gap: ".35rem" }}>
                {candidates.map((c) => (
                  <label key={c.id} className="inline-choice">
                    <input type="radio" name="candidate" checked={chosen === c.id} onChange={() => setChosen(c.id)} />
                    C'est cette personne : {c.first_name} {c.last_name} — {c.job_title ?? "poste ?"}, {c.site_name ?? "site ?"}
                    {c.matricule && ` — matricule ${c.matricule}`} ({employeeStatusLabel[c.status]})
                    {c.same_matricule && <strong> · même matricule</strong>}
                  </label>
                ))}
                <label className="inline-choice">
                  <input type="radio" name="candidate" checked={chosen === "new"} onChange={() => setChosen("new")} />
                  Nouvelle personne (homonyme)
                </label>
              </div>
            </Notice>
          )}

          <details>
            <summary>Informations facultatives (matricule, département, manager, note RH)</summary>
            <div className="form-grid">
              <Field label="Matricule" hint="Peut être attribué plus tard." error={error?.detail?.field === "matricule" ? error.message : null}>
                <input value={optional.matricule} onChange={(e) => setOptional({ ...optional, matricule: e.target.value })} />
              </Field>
              <Field label="Département">
                <input value={optional.department} onChange={(e) => setOptional({ ...optional, department: e.target.value })} />
              </Field>
              <Field label="Manager">
                <input value={optional.manager} onChange={(e) => setOptional({ ...optional, manager: e.target.value })} />
              </Field>
            </div>
            <Field label="Note RH (visible du RH uniquement)">
              <textarea rows={2} value={optional.note} onChange={(e) => setOptional({ ...optional, note: e.target.value })} />
            </Field>
          </details>
        </section>

        <section className="card">
          <details>
            <summary>Préciser les besoins maintenant</summary>
            <NeedsEditor needs={needs} onChange={setNeeds} trainings={trainings} onTrainings={setTrainings} />
          </details>
        </section>

        <ErrorBox error={error} />
        <div className="card row" style={{ justifyContent: "space-between" }}>
          <span>
            Seront prévenus immédiatement : <strong>{recipients(needs, trainings).join(", ")}</strong>
            {form.planned_arrival && <span className="muted"> — arrivée le {fmtDate(form.planned_arrival)}</span>}
          </span>
          <button className="btn btn-primary" type="submit" disabled={busy}>
            {busy ? "Création…" : "Créer et prévenir les équipes"}
          </button>
        </div>
      </form>
    </>
  );
}
