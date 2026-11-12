import { FormEvent, useMemo, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { useAuth } from "../auth";
import { ErrorBox, Field, Loading, Notice, PageHeader, Section } from "../components";
import { useLoad, useUnsavedWarning } from "../hooks";
import { assetKindLabel, assetStatusLabel, preferenceLabel } from "../labels";
import { DocumentActions } from "./DocumentActions";

type Line =
  | { type: "asset"; asset_id: string; label: string; identifier: string; kind: string; condition: string }
  | { type: "accessoire"; accessory_id: string; label: string; quantity: number; condition: string }
  | { type: "sim"; sim_id: string; label: string; identifier: string };

const SUBSTITUTIONS = ["Plus de PC fixe disponible en stock", "Poste nomade confirmé avec le manager", "Matériel préféré en réparation"];

/** Remise groupée : scanner ou choisir, résumé, puis confirmation unique. */
export default function HandoverPage() {
  const { has } = useAuth();
  const [params] = useSearchParams();
  const employeeId = params.get("salarie") ?? "";
  const caseId = params.get("dossier");
  const taskId = params.get("tache");
  const employee = useLoad(() => api<any>(`/employees/${employeeId}`), [employeeId]);
  const caseInfo = useLoad(() => (caseId ? api<any>(`/cases/${caseId}`) : Promise.resolve(null)), [caseId]);
  const assets = useLoad(() => api<any[]>("/assets?status=disponible"), []);
  const accessories = useLoad(() => api<any[]>("/accessories"), []);
  const sims = useLoad(() => api<any[]>("/sims"), []);

  const [lines, setLines] = useState<Line[]>([]);
  const [scan, setScan] = useState("");
  const [scanMsg, setScanMsg] = useState<{ kind: "ok" | "warn" | "danger"; text: string } | null>(null);
  const [acc, setAcc] = useState({ id: "", qty: 1 });
  const [substitution, setSubstitution] = useState("");
  const [note, setNote] = useState("");
  const [key] = useState(newKey);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [result, setResult] = useState<{ reference: string; document_id: string } | null>(null);
  const scanRef = useRef<HTMLInputElement>(null);
  useUnsavedWarning(lines.length > 0 && !result);

  // La préférence PC du dossier vaut pour toute remise du dossier ; toutes
  // les lignes sont examinées, quel que soit leur ordre.
  const pcNeed = caseInfo.data?.needs?.find((n: any) => n.category === "pc" && n.answer === "oui");
  const preference: string | null = pcNeed?.preference ?? null;
  const mismatch = useMemo(() => {
    const opposite = preference === "fixe" ? "pc_portable" : preference === "portable" ? "pc_fixe" : null;
    return !!opposite && lines.some((l) => l.type === "asset" && l.kind === opposite);
  }, [preference, lines]);

  if (!has("IT", "ADMIN")) return <Notice kind="danger">Réservé à l'IT.</Notice>;
  if (!employeeId) return <Notice kind="warn">Ouvrez la remise depuis la fiche salarié ou depuis la tâche du dossier.</Notice>;
  if (employee.loading && !employee.data) return <Loading />;
  if (employee.error) return <ErrorBox error={employee.error} />;
  const e = employee.data.employee;

  const addAsset = (a: any) => {
    if (lines.some((l) => l.type === "asset" && l.asset_id === a.id)) {
      setScanMsg({ kind: "warn", text: `${a.inventory_no} est déjà dans la remise.` });
      return;
    }
    if (!["disponible", "reserve"].includes(a.status)) {
      setScanMsg({ kind: "danger", text: `${a.inventory_no} n'est pas remettable : ${assetStatusLabel[a.status]}${a.holder_name ? ` (${a.holder_name})` : ""}.` });
      return;
    }
    setLines([...lines, { type: "asset", asset_id: a.id, label: a.label, identifier: a.serial_no ? `${a.inventory_no} / ${a.serial_no}` : a.inventory_no, kind: a.kind, condition: "" }]);
    setScanMsg({ kind: "ok", text: `Ajouté : ${a.inventory_no} — ${a.label}` });
  };

  const onScan = async (ev: FormEvent) => {
    ev.preventDefault();
    const code = scan.trim();
    if (!code) return;
    setScan("");
    try {
      addAsset(await api(`/assets/lookup/${encodeURIComponent(code)}`));
    } catch (err) {
      setScanMsg({ kind: "danger", text: (err as ApiError).message });
    }
    scanRef.current?.focus();
  };

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      const body = {
        employee_id: employeeId,
        case_id: caseId,
        task_id: taskId,
        substitution_reason: substitution || null,
        note: note || null,
        lines: lines.map((l) =>
          l.type === "asset"
            ? { type: "asset", asset_id: l.asset_id, condition: l.condition || null }
            : l.type === "accessoire"
              ? { type: "accessoire", accessory_id: l.accessory_id, quantity: l.quantity, condition: l.condition || null }
              : { type: "sim", sim_id: l.sim_id },
        ),
      };
      setResult(await api("/handovers", { body, idempotencyKey: key }));
    } catch (err) {
      setError(err as ApiError);
    } finally {
      setBusy(false);
    }
  };

  if (result) {
    return (
      <>
        <PageHeader title="Remise enregistrée" />
        <div className="success-panel stack">
          <div className="ref">{result.reference}</div>
          <p>
            {lines.length} élément(s) remis à {e.first_name} {e.last_name}. Imprimez la décharge, faites-la signer puis déposez le scan.
          </p>
          <DocumentActions documentId={result.document_id} status="genere" onChange={() => undefined} />
          <div className="row">
            {caseInfo.data && (
              <Link className="btn btn-primary" to={`/dossiers/${caseInfo.data.case.reference}`}>
                Retour au dossier {caseInfo.data.case.reference}
              </Link>
            )}
            <Link className="btn" to={`/personnel/${employeeId}`}>
              Fiche salarié
            </Link>
          </div>
        </div>
      </>
    );
  }

  const freeSims = sims.data?.filter((s) => s.status === "disponible" && !lines.some((l) => l.type === "sim" && l.sim_id === s.id)) ?? [];

  return (
    <>
      <PageHeader
        title={`Remise — ${e.first_name} ${e.last_name}`}
        subtitle={
          <>
            {e.job_title} · {e.site_name}
            {caseInfo.data && ` · dossier ${caseInfo.data.case.reference}`}
          </>
        }
      />
      {preference && <Notice kind="info">Demandé : {preferenceLabel[preference]}</Notice>}
      <div className="grid-2">
        <div className="stack">
          <Section title="Scanner ou saisir un actif">
            <form onSubmit={onScan}>
              <input
                ref={scanRef}
                className="scan-input"
                value={scan}
                onChange={(ev) => setScan(ev.target.value)}
                placeholder="QR ou n° d'inventaire puis Entrée"
                aria-label="Code de l'actif"
                autoFocus
              />
            </form>
            {scanMsg && <div className={`alert alert-${scanMsg.kind}`} role="status">{scanMsg.text}</div>}
            <Field label="Ou choisir un actif disponible">
              <select
                value=""
                onChange={(ev) => {
                  const a = assets.data?.find((x) => x.id === ev.target.value);
                  if (a) addAsset(a);
                }}
              >
                <option value="">Choisir…</option>
                {assets.data
                  ?.filter((a) => !lines.some((l) => l.type === "asset" && l.asset_id === a.id))
                  .map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.inventory_no} — {a.label} ({assetKindLabel[a.kind]}, {a.site_name ?? "site ?"})
                    </option>
                  ))}
              </select>
            </Field>
          </Section>
          <Section title="Accessoires et SIM">
            <div className="row" style={{ alignItems: "flex-end" }}>
              <Field label="Accessoire">
                <select value={acc.id} onChange={(ev) => setAcc({ ...acc, id: ev.target.value })}>
                  <option value="">Choisir…</option>
                  {accessories.data?.map((a) => (
                    <option key={a.id} value={a.id} disabled={a.quantity_on_hand < 1}>
                      {a.label} ({a.quantity_on_hand} en stock)
                    </option>
                  ))}
                </select>
              </Field>
              <Field label="Qté">
                <input type="number" min={1} value={acc.qty} onChange={(ev) => setAcc({ ...acc, qty: Number(ev.target.value) })} style={{ width: 80 }} />
              </Field>
              <button
                type="button"
                className="btn"
                style={{ marginBottom: ".8rem" }}
                disabled={!acc.id}
                onClick={() => {
                  const a = accessories.data?.find((x) => x.id === acc.id);
                  if (!a) return;
                  setLines([...lines, { type: "accessoire", accessory_id: a.id, label: a.label, quantity: acc.qty, condition: "" }]);
                  setAcc({ id: "", qty: 1 });
                }}
              >
                Ajouter
              </button>
            </div>
            <Field label="Carte SIM / ligne">
              <select
                value=""
                onChange={(ev) => {
                  const s = sims.data?.find((x) => x.id === ev.target.value);
                  if (s) setLines([...lines, { type: "sim", sim_id: s.id, label: s.line_number ? `SIM — ligne ${s.line_number}` : "SIM", identifier: s.iccid }]);
                }}
              >
                <option value="">{freeSims.length ? "Choisir…" : "Aucune SIM disponible"}</option>
                {freeSims.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.line_number ?? "sans ligne"} — {s.iccid} {s.operator && `(${s.operator})`}
                  </option>
                ))}
              </select>
            </Field>
          </Section>
        </div>

        <Section title={`Récapitulatif (${lines.length})`}>
          {lines.length === 0 ? (
            <p className="empty">Aucun élément.</p>
          ) : (
            <table>
              <tbody>
                {lines.map((l, i) => (
                  <tr key={i}>
                    <td>
                      <strong>{l.label}</strong>
                      {"identifier" in l && <div className="mono small">{l.identifier}</div>}
                      {l.type === "accessoire" && <div className="small">Quantité : {l.quantity}</div>}
                    </td>
                    <td>
                      {l.type !== "sim" && (
                        <input
                          aria-label="État à la remise"
                          placeholder="État à la remise"
                          value={l.condition}
                          onChange={(ev) => setLines(lines.map((x, j) => (j === i ? ({ ...x, condition: ev.target.value } as Line) : x)))}
                        />
                      )}
                    </td>
                    <td>
                      <button className="btn btn-small" onClick={() => setLines(lines.filter((_, j) => j !== i))} aria-label={`Retirer ${l.label}`}>
                        Retirer
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {mismatch && (
            <Field label="Raison de la substitution" required hint="La préférence RH reste enregistrée telle quelle.">
              <input list="substitutions" value={substitution} onChange={(ev) => setSubstitution(ev.target.value)} />
              <datalist id="substitutions">
                {SUBSTITUTIONS.map((s) => (
                  <option key={s} value={s} />
                ))}
              </datalist>
            </Field>
          )}
          <Field label="Accessoires, réserves et observations">
            <textarea rows={2} value={note} onChange={(ev) => setNote(ev.target.value)} />
          </Field>
          <ErrorBox error={error} />
          <div className="form-actions">
            <button className="btn btn-primary" disabled={busy || lines.length === 0 || (mismatch && !substitution.trim())} onClick={confirm}>
              {busy ? "Enregistrement…" : `Confirmer la remise (${lines.length})`}
            </button>
          </div>
        </Section>
      </div>
    </>
  );
}
