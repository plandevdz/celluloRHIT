import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { useAuth } from "../auth";
import { Empty, ErrorBox, Field, Loading, Notice, PageHeader, Section } from "../components";
import { useLoad, useUnsavedWarning } from "../hooks";
import { fmtDateTime } from "../labels";
import { DocumentActions } from "./DocumentActions";

type Pick = { checked: boolean; quantity: number; condition: string };

/** Restitution ligne par ligne : seules les quantités rendues sont soldées. */
export default function ReturnPage() {
  const { has } = useAuth();
  const [params] = useSearchParams();
  const employeeId = params.get("salarie") ?? "";
  const caseId = params.get("dossier");
  const employee = useLoad(() => api<any>(`/employees/${employeeId}`), [employeeId]);
  const [picks, setPicks] = useState<Record<string, Pick>>({});
  const [note, setNote] = useState("");
  const [key] = useState(newKey);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [result, setResult] = useState<{ reference: string; document_id: string } | null>(null);
  const selected = Object.entries(picks).filter(([, p]) => p.checked);
  useUnsavedWarning(selected.length > 0 && !result);

  if (!has("IT", "ADMIN")) return <Notice kind="danger">Réservé à l'IT.</Notice>;
  if (employee.loading && !employee.data) return <Loading />;
  if (employee.error) return <ErrorBox error={employee.error} />;
  const { employee: e, holdings } = employee.data;

  const pick = (h: any): Pick => picks[h.origin_line_id] ?? { checked: false, quantity: h.open_quantity, condition: "" };
  const update = (h: any, patch: Partial<Pick>) => setPicks({ ...picks, [h.origin_line_id]: { ...pick(h), ...patch } });

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(
        await api("/returns", {
          idempotencyKey: key,
          body: {
            employee_id: employeeId,
            case_id: caseId,
            note: note || null,
            lines: selected.map(([origin, p]) => ({ origin_line_id: origin, quantity: p.quantity, condition: p.condition || null })),
          },
        }),
      );
    } catch (err) {
      setError(err as ApiError);
    } finally {
      setBusy(false);
    }
  };

  if (result) {
    const remaining = holdings.length - selected.filter(([o, p]) => holdings.find((h: any) => h.origin_line_id === o)?.open_quantity === p.quantity).length;
    return (
      <>
        <PageHeader title="Restitution enregistrée" />
        <div className="success-panel stack">
          <div className="ref">{result.reference}</div>
          <p>{remaining > 0 ? `${remaining} élément(s) restent détenus par le salarié et suivis séparément.` : "Plus aucun bien détenu."}</p>
          <DocumentActions documentId={result.document_id} status="genere" onChange={() => undefined} />
          <Link className="btn" to={`/personnel/${employeeId}`}>
            Retour à la fiche
          </Link>
        </div>
      </>
    );
  }

  return (
    <>
      <PageHeader title={`Restitution — ${e.first_name} ${e.last_name}`} subtitle="Cochez les seuls éléments effectivement rendus." />
      <Section title="Biens encore détenus">
        {holdings.length === 0 ? (
          <Empty>Aucun bien détenu.</Empty>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Rendu</th>
                <th>Élément</th>
                <th>Remise d'origine</th>
                <th className="num">Qté rendue</th>
                <th>État au retour</th>
              </tr>
            </thead>
            <tbody>
              {holdings.map((h: any) => {
                const p = pick(h);
                return (
                  <tr key={h.origin_line_id}>
                    <td>
                      <input type="checkbox" aria-label={`Rendu : ${h.label}`} checked={p.checked} onChange={(ev) => update(h, { checked: ev.target.checked })} />
                    </td>
                    <td>
                      <strong>{h.label}</strong>
                      {h.identifier && <div className="mono small">{h.identifier}</div>}
                    </td>
                    <td className="small">
                      {h.origin_reference}
                      <div className="muted">{fmtDateTime(h.since)}</div>
                    </td>
                    <td className="num">
                      {h.item_type === "accessoire" && h.open_quantity > 1 ? (
                        <input
                          type="number"
                          min={1}
                          max={h.open_quantity}
                          value={p.quantity}
                          disabled={!p.checked}
                          onChange={(ev) => update(h, { quantity: Number(ev.target.value) })}
                          style={{ width: 70 }}
                        />
                      ) : (
                        h.open_quantity
                      )}
                      <span className="muted small"> / {h.open_quantity}</span>
                    </td>
                    <td>
                      <input
                        aria-label={`État au retour : ${h.label}`}
                        value={p.condition}
                        disabled={!p.checked}
                        onChange={(ev) => update(h, { condition: ev.target.value })}
                        placeholder="Bon état, rayé…"
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        <Field label="Éléments restants, manquants, détériorés et suites convenues">
          <textarea rows={2} value={note} onChange={(ev) => setNote(ev.target.value)} />
        </Field>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button className="btn btn-primary" disabled={busy || selected.length === 0} onClick={confirm}>
            Confirmer la restitution ({selected.length})
          </button>
        </div>
      </Section>
    </>
  );
}
