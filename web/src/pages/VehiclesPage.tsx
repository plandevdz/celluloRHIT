import { FormEvent, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { Badge, Empty, EmployeePicker, ErrorBox, Field, Loading, Modal, Notice, PageHeader, Section, Tabs } from "../components";
import { useCommand, useLoad } from "../hooks";
import {
  fmtDate,
  fmtDateTime,
  fromLocalInput,
  leaseStatusLabel,
  maintenanceLabel,
  todayIso,
  vehicleDocLabel,
  vehicleKindLabel,
  vehicleMovementLabel,
  vehicleStatusLabel,
} from "../labels";

const km = (n?: number | null) => (n === null || n === undefined ? "—" : `${n.toLocaleString("fr-FR")} km`);

/** Parc véhicules RH : propriété ou location, statut, détenteur, échéances. */
export default function VehiclesPage() {
  const [params, setParams] = useSearchParams();
  const kind = params.get("type") ?? "";
  const status = params.get("etat") ?? "";
  const [q, setQ] = useState(params.get("q") ?? "");
  const query = new URLSearchParams(Object.entries({ kind, status, q: params.get("q") ?? "" }).filter(([, v]) => v));
  const list = useLoad(() => api<any[]>(`/vehicles?${query}`), [query.toString()]);
  const deadlines = useLoad(() => api<any[]>("/vehicles/deadlines"), []);
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();
  const forEmployee = params.get("salarie");
  const assignQs = forEmployee
    ? `?${new URLSearchParams({ salarie: forEmployee, nom: params.get("nom") ?? "", dossier: params.get("dossier") ?? "", tache: params.get("tache") ?? "", remise: "1" })}`
    : "";
  const set = (k: string, v: string) => {
    const p = new URLSearchParams(params);
    if (v) p.set(k, v);
    else p.delete(k);
    setParams(p, { replace: true });
  };

  return (
    <>
      <PageHeader
        title="Véhicules"
        subtitle="Propres ou loués : titulaire, kilométrage, prochaines échéances."
        actions={
          <button className="btn btn-primary" onClick={() => setCreating(true)}>
            + Nouveau véhicule
          </button>
        }
      />
      {forEmployee && (
        <Notice kind="info">
          Remise demandée pour <strong>{params.get("nom")}</strong> : choisissez un véhicule disponible.
        </Notice>
      )}
      <div className="stack">
        {(deadlines.data?.length ?? 0) > 0 && (
          <Section title={`Échéances à traiter (${deadlines.data!.length})`}>
            <table>
              <thead>
                <tr>
                  <th>Véhicule</th>
                  <th>Échéance</th>
                  <th>Date / km</th>
                  <th>Titulaire</th>
                </tr>
              </thead>
              <tbody>
                {deadlines.data!.map((d) => (
                  <tr key={`${d.source}-${d.id}`} className={d.overdue ? "row-warn" : ""}>
                    <td>
                      <Link to={`/vehicules/${d.vehicle_id}`}>{d.plate}</Link>
                    </td>
                    <td>
                      {vehicleDocLabel[d.kind] ?? maintenanceLabel[d.kind] ?? (d.kind === "fin_location" ? "Fin de location" : d.kind)}
                      {d.overdue && (
                        <>
                          {" "}
                          <Badge status="echec" label="Dépassée" />
                        </>
                      )}
                    </td>
                    <td className="small">
                      {d.due_date && fmtDate(d.due_date)}
                      {d.due_km && ` · ${km(d.due_km)} (compteur ${km(d.odometer_km)})`}
                    </td>
                    <td className="small">{d.holder ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Section>
        )}
        <Section title={`Parc (${list.data?.length ?? 0})`}>
          <div className="filters">
            <label className="field">
              <span className="field-label">Type</span>
              <select value={kind} onChange={(e) => set("type", e.target.value)}>
                <option value="">Tous</option>
                <option value="location">Loués</option>
                <option value="propriete">Propres</option>
              </select>
            </label>
            <label className="field">
              <span className="field-label">État</span>
              <select value={status} onChange={(e) => set("etat", e.target.value)}>
                <option value="">Tous</option>
                {Object.entries(vehicleStatusLabel).map(([k, l]) => (
                  <option key={k} value={k}>
                    {l}
                  </option>
                ))}
              </select>
            </label>
            <form
              className="field"
              onSubmit={(e) => {
                e.preventDefault();
                set("q", q);
              }}
            >
              <span className="field-label">Recherche</span>
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Immatriculation, modèle, titulaire" />
            </form>
          </div>
          <ErrorBox error={list.error} onRetry={list.reload} />
          {list.loading && !list.data ? (
            <Loading />
          ) : list.data?.length === 0 ? (
            <Empty>Aucun véhicule.</Empty>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Immatriculation</th>
                    <th>Véhicule</th>
                    <th>Type</th>
                    <th>État</th>
                    <th>Titulaire</th>
                    <th className="num">Compteur</th>
                    <th>Assurance · CT</th>
                  </tr>
                </thead>
                <tbody>
                  {list.data?.map((v) => (
                    <tr key={v.id}>
                      <td className="mono">
                        <Link to={`/vehicules/${v.id}${v.status === "disponible" ? assignQs : ""}`}>{v.plate}</Link>
                      </td>
                      <td>
                        {[v.brand, v.model].filter(Boolean).join(" ") || "—"}
                        {v.site_name && <div className="small muted">{v.site_name}</div>}
                      </td>
                      <td className="small">
                        {vehicleKindLabel[v.kind]}
                        {v.lessor && <div className="muted">{v.lessor} · fin {fmtDate(v.lease_end)}</div>}
                      </td>
                      <td>
                        <Badge status={v.status} label={vehicleStatusLabel[v.status]} />
                      </td>
                      <td>{v.holder_id ? <Link to={`/personnel/${v.holder_id}`}>{v.holder_name}</Link> : "—"}</td>
                      <td className="num small">{km(v.odometer_km)}</td>
                      <td className="small">
                        {fmtDate(v.insurance_until)} · {fmtDate(v.inspection_until)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Section>
      </div>
      {creating && <CreateVehicleDialog onClose={() => setCreating(false)} onCreated={(id) => navigate(`/vehicules/${id}`)} />}
    </>
  );
}

function CreateVehicleDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({ plate: "", kind: "propriete", brand: "", model: "", site_id: "", odometer_km: "", notes: "", lessor: "", contract_no: "", start_date: "", end_date: "" });
  const [error, setError] = useState<ApiError | null>(null);
  const bind = (k: keyof typeof f) => ({ value: f[k], onChange: (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value }) });
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      const r = await api<{ id: string }>("/vehicles", {
        body: {
          plate: f.plate,
          kind: f.kind,
          brand: f.brand || null,
          model: f.model || null,
          site_id: f.site_id || null,
          odometer_km: f.odometer_km ? Number(f.odometer_km) : null,
          notes: f.notes || null,
          lease:
            f.kind === "location"
              ? { lessor: f.lessor, contract_no: f.contract_no || null, start_date: f.start_date || null, end_date: f.end_date || null }
              : null,
        },
      });
      onCreated(r.id);
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title="Nouveau véhicule" onClose={onClose} wide>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="Immatriculation" required>
            <input {...bind("plate")} required className="mono" />
          </Field>
          <Field label="Type" required>
            <select {...bind("kind")}>
              <option value="propriete">Propriété</option>
              <option value="location">Location</option>
            </select>
          </Field>
          <Field label="Marque">
            <input {...bind("brand")} />
          </Field>
          <Field label="Modèle">
            <input {...bind("model")} />
          </Field>
          <Field label="Site">
            <select {...bind("site_id")}>
              <option value="">—</option>
              {sites.data?.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Kilométrage actuel">
            <input type="number" min={0} {...bind("odometer_km")} />
          </Field>
        </div>
        {f.kind === "location" && (
          <fieldset>
            <legend>Contrat de location</legend>
            <div className="form-grid">
              <Field label="Loueur" required>
                <input {...bind("lessor")} required />
              </Field>
              <Field label="N° de contrat">
                <input {...bind("contract_no")} />
              </Field>
              <Field label="Début">
                <input type="date" {...bind("start_date")} />
              </Field>
              <Field label="Fin de contrat">
                <input type="date" {...bind("end_date")} />
              </Field>
            </div>
          </fieldset>
        )}
        <Field label="Observations">
          <textarea rows={2} {...bind("notes")} />
        </Field>
        <ErrorBox error={error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            Enregistrer
          </button>
        </div>
      </form>
    </Modal>
  );
}

const TABS = [
  { id: "mouvements", label: "Mouvements" },
  { id: "documents", label: "Documents" },
  { id: "contrat", label: "Contrat" },
  { id: "entretien", label: "Entretien" },
];

export function VehiclePage() {
  const { id = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const tab = params.get("onglet") ?? "mouvements";
  const d = useLoad(() => api<any>(`/vehicles/${id}`), [id]);
  const [dialog, setDialog] = useState<null | "remise" | "restitution" | "loueur" | "document" | "entretien" | "contrat" | "etat">(
    params.get("remise") === "1" ? "remise" : null,
  );
  const prefill = params.get("salarie") ? { id: params.get("salarie")!, name: params.get("nom") ?? "" } : null;
  if (d.loading && !d.data) return <Loading />;
  if (d.error) return <ErrorBox error={d.error} onRetry={d.reload} />;
  const { vehicle: v, holder, leases, documents, maintenance, movements } = d.data;
  const done = () => {
    setDialog(null);
    d.reload();
  };
  const currentLease = leases.find((l: any) => l.status === "en_cours");
  return (
    <>
      <PageHeader
        title={
          <>
            <span className="mono">{v.plate}</span> — {[v.brand, v.model].filter(Boolean).join(" ") || "Véhicule"}
          </>
        }
        subtitle={
          <>
            {vehicleKindLabel[v.kind]} · <Badge status={v.status} label={vehicleStatusLabel[v.status]} /> · compteur {km(v.odometer_km)}
            {v.site_name && ` · ${v.site_name}`}
          </>
        }
        actions={
          <>
            {v.status === "disponible" && (
              <button className="btn btn-primary" onClick={() => setDialog("remise")}>
                Remettre à un salarié
              </button>
            )}
            {holder && (
              <button className="btn btn-primary" onClick={() => setDialog("restitution")}>
                Enregistrer la restitution
              </button>
            )}
            {v.kind === "location" && !holder && v.status !== "rendu_loueur" && (
              <button className="btn" onClick={() => setDialog("loueur")}>
                Retour au loueur
              </button>
            )}
            {!["affecte", "rendu_loueur"].includes(v.status) && (
              <button className="btn" onClick={() => setDialog("etat")}>
                Changer l'état
              </button>
            )}
          </>
        }
      />
      {holder && (
        <Notice kind="info">
          Confié à <Link to={`/personnel/${holder.employee_id}`}>{holder.name}</Link> depuis le {fmtDateTime(holder.since)} ({holder.handover_reference}).
        </Notice>
      )}
      {v.status === "rendu_loueur" && <Notice kind="warn">Véhicule rendu au loueur : il ne peut plus être affecté.</Notice>}
      {v.notes && <p className="small">Observations : {v.notes}</p>}
      <Tabs tabs={TABS} current={tab} onSelect={(t) => setParams({ onglet: t }, { replace: true })} />
      {tab === "mouvements" && (
        <Section title="Mouvements">
          {movements.length === 0 ? (
            <Empty>Aucun mouvement.</Empty>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>Mouvement</th>
                  <th>Salarié</th>
                  <th>Date</th>
                  <th className="num">Km</th>
                  <th>État · clés · carburant</th>
                </tr>
              </thead>
              <tbody>
                {movements.map((m: any) => (
                  <tr key={m.id}>
                    <td>
                      <strong>{vehicleMovementLabel[m.kind]}</strong>
                      <div className="small mono">{m.reference}</div>
                      {m.case_reference && (
                        <Link className="small" to={`/dossiers/${m.case_reference}`}>
                          {m.case_reference}
                        </Link>
                      )}
                    </td>
                    <td>{m.employee_id ? <Link to={`/personnel/${m.employee_id}`}>{m.employee}</Link> : "—"}</td>
                    <td className="small">
                      {fmtDateTime(m.effective_at)}
                      <div className="muted">{m.operator}</div>
                    </td>
                    <td className="num small">
                      {km(m.odometer_km)}
                      {m.odometer_reason && <div className="muted">corrigé : {m.odometer_reason}</div>}
                    </td>
                    <td className="small">
                      {[m.condition, m.keys_count !== null && m.keys_count !== undefined ? `${m.keys_count} clé(s)` : null, m.fuel_level]
                        .filter(Boolean)
                        .join(" · ") || "—"}
                      {m.note && <div className="muted">{m.note}</div>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Section>
      )}
      {tab === "documents" && (
        <Section title="Pièces" actions={<button className="btn btn-primary" onClick={() => setDialog("document")}>+ Pièce ou renouvellement</button>}>
          {documents.length === 0 ? (
            <Empty>Aucune pièce enregistrée.</Empty>
          ) : (
            <table>
              <tbody>
                {documents.map((x: any) => (
                  <tr key={x.id} className={x.current ? "" : "muted"}>
                    <td>
                      <strong>{vehicleDocLabel[x.kind]}</strong> {x.current ? <Badge status="termine" label="En cours" /> : <span className="tag">Historique</span>}
                    </td>
                    <td className="small">{x.reference ?? "—"}</td>
                    <td className="small">
                      {fmtDate(x.valid_from)} → {fmtDate(x.valid_until)}
                    </td>
                    <td className="small muted">{x.note}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Section>
      )}
      {tab === "contrat" && (
        <Section
          title="Contrats de location"
          actions={
            v.kind === "location" && v.status !== "rendu_loueur" ? (
              <button className="btn btn-primary" onClick={() => setDialog("contrat")}>
                {currentLease ? "Renouveler le contrat" : "+ Contrat"}
              </button>
            ) : undefined
          }
        >
          {v.kind !== "location" ? (
            <Empty>Véhicule en propriété : aucun contrat de location.</Empty>
          ) : leases.length === 0 ? (
            <Empty>Aucun contrat.</Empty>
          ) : (
            <table>
              <tbody>
                {leases.map((l: any) => (
                  <tr key={l.id}>
                    <td>
                      <strong>{l.lessor}</strong>
                      <div className="small mono">{l.contract_no ?? "—"}</div>
                    </td>
                    <td className="small">
                      {fmtDate(l.start_date)} → {fmtDate(l.end_date)}
                    </td>
                    <td>
                      <Badge status={l.status} label={leaseStatusLabel[l.status]} />
                      {l.returned_on && <div className="small">rendu le {fmtDate(l.returned_on)}</div>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Section>
      )}
      {tab === "entretien" && (
        <Section title="Entretien" actions={<button className="btn btn-primary" onClick={() => setDialog("entretien")}>+ Intervention</button>}>
          {maintenance.length === 0 ? (
            <Empty>Aucune intervention.</Empty>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>Intervention</th>
                  <th>Date · km</th>
                  <th>Prochaine échéance</th>
                </tr>
              </thead>
              <tbody>
                {maintenance.map((m: any) => (
                  <tr key={m.id}>
                    <td>
                      <strong>{maintenanceLabel[m.kind]}</strong>
                      {m.description && <div className="small">{m.description}</div>}
                      {m.provider && <div className="small muted">{m.provider}</div>}
                    </td>
                    <td className="small">
                      {fmtDate(m.done_on)} · {km(m.odometer_km)}
                    </td>
                    <td className="small">
                      {m.next_due_date ? fmtDate(m.next_due_date) : ""}
                      {m.next_due_date && m.next_due_km ? " ou " : ""}
                      {m.next_due_km ? km(m.next_due_km) : ""}
                      {!m.next_due_date && !m.next_due_km && "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Section>
      )}
      {dialog === "remise" && v.status === "disponible" && (
        <HandoverDialog vehicle={v} prefill={prefill} caseId={params.get("dossier") || null} taskId={params.get("tache") || null} onClose={() => setDialog(null)} onDone={done} />
      )}
      {dialog === "restitution" && <ReturnDialog vehicle={v} holder={holder} onClose={() => setDialog(null)} onDone={done} />}
      {dialog === "loueur" && <LessorReturnDialog vehicle={v} onClose={() => setDialog(null)} onDone={done} />}
      {dialog === "document" && <DocumentDialog vehicle={v} onClose={() => setDialog(null)} onDone={done} />}
      {dialog === "entretien" && <MaintenanceDialog vehicle={v} onClose={() => setDialog(null)} onDone={done} />}
      {dialog === "contrat" && <LeaseDialog vehicle={v} renew={!!currentLease} onClose={() => setDialog(null)} onDone={done} />}
      {dialog === "etat" && <StatusDialog vehicle={v} onClose={() => setDialog(null)} onDone={done} />}
    </>
  );
}

/** Champ kilométrage avec justification si le relevé baisse. */
function OdometerFields({ last, value, reason, onValue, onReason, required = true }: { last?: number | null; value: string; reason: string; onValue: (v: string) => void; onReason: (v: string) => void; required?: boolean }) {
  const lower = last !== null && last !== undefined && value !== "" && Number(value) < last;
  return (
    <>
      <Field label="Kilométrage relevé" required={required} hint={last !== null && last !== undefined ? `Dernier relevé : ${km(last)}` : undefined}>
        <input type="number" min={0} value={value} onChange={(e) => onValue(e.target.value)} required={required} />
      </Field>
      {lower && (
        <Field label="Justification de l'écart de compteur" required error="Relevé inférieur au dernier connu : justifiez ou corrigez.">
          <input value={reason} onChange={(e) => onReason(e.target.value)} required />
        </Field>
      )}
    </>
  );
}

function HandoverDialog({ vehicle, prefill, caseId, taskId, onClose, onDone }: { vehicle: any; prefill: { id: string; name: string } | null; caseId: string | null; taskId: string | null; onClose: () => void; onDone: () => void }) {
  const [emp, setEmp] = useState(prefill);
  const [f, setF] = useState({ odometer: vehicle.odometer_km?.toString() ?? "", reason: "", condition: "", keys: "", fuel: "", note: "", at: "" });
  const [key] = useState(newKey);
  const cmd = useCommand();
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!emp) return;
    const r = await cmd.run(() =>
      api(`/vehicles/${vehicle.id}/handover`, {
        idempotencyKey: key,
        body: {
          employee_id: emp.id,
          case_id: caseId,
          task_id: taskId,
          effective_at: fromLocalInput(f.at),
          odometer_km: Number(f.odometer),
          odometer_reason: f.reason || null,
          condition: f.condition || null,
          keys_count: f.keys ? Number(f.keys) : null,
          fuel_level: f.fuel || null,
          note: f.note || null,
        },
      }),
    );
    if (r) onDone();
  };
  return (
    <Modal title={`Remise du véhicule ${vehicle.plate}`} onClose={onClose} wide>
      <form onSubmit={submit}>
        <EmployeePicker value={emp} onChange={setEmp} required />
        {caseId && <p className="small muted">Remise rattachée au dossier en cours.</p>}
        <div className="form-grid">
          <OdometerFields last={vehicle.odometer_km} value={f.odometer} reason={f.reason} onValue={(v) => setF({ ...f, odometer: v })} onReason={(v) => setF({ ...f, reason: v })} />
          <Field label="Nombre de clés">
            <input type="number" min={0} value={f.keys} onChange={(e) => setF({ ...f, keys: e.target.value })} />
          </Field>
          <Field label="Carburant">
            <input value={f.fuel} onChange={(e) => setF({ ...f, fuel: e.target.value })} placeholder="ex. 1/2" />
          </Field>
          <Field label="Date et heure (Alger)" hint="Vide : maintenant">
            <input type="datetime-local" value={f.at} onChange={(e) => setF({ ...f, at: e.target.value })} />
          </Field>
        </div>
        <Field label="État constaté">
          <textarea rows={2} value={f.condition} onChange={(e) => setF({ ...f, condition: e.target.value })} />
        </Field>
        <Field label="Observations">
          <input value={f.note} onChange={(e) => setF({ ...f, note: e.target.value })} />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={!emp || cmd.busy}>
            Enregistrer la remise
          </button>
        </div>
      </form>
    </Modal>
  );
}

function ReturnDialog({ vehicle, holder, onClose, onDone }: { vehicle: any; holder: any; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ odometer: "", reason: "", condition: "", keys: "", fuel: "", note: "", at: "", destination: "celluloplast" });
  const [key] = useState(newKey);
  const cmd = useCommand();
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const r = await cmd.run(() =>
      api(`/vehicles/${vehicle.id}/return`, {
        idempotencyKey: key,
        body: {
          effective_at: fromLocalInput(f.at),
          odometer_km: Number(f.odometer),
          odometer_reason: f.reason || null,
          condition: f.condition || null,
          keys_count: f.keys ? Number(f.keys) : null,
          fuel_level: f.fuel || null,
          note: f.note || null,
          destination: vehicle.kind === "location" ? f.destination : "celluloplast",
        },
      }),
    );
    if (r) onDone();
  };
  return (
    <Modal title={`Restitution du véhicule ${vehicle.plate} par ${holder.name}`} onClose={onClose} wide>
      <form onSubmit={submit}>
        {vehicle.kind === "location" && (
          <fieldset>
            <legend>Le véhicule revient…</legend>
            <label className="choice-row">
              <input type="radio" checked={f.destination === "celluloplast"} onChange={() => setF({ ...f, destination: "celluloplast" })} />
              <span>chez Celluloplast (reste disponible)</span>
            </label>
            <label className="choice-row">
              <input type="radio" checked={f.destination === "loueur"} onChange={() => setF({ ...f, destination: "loueur" })} />
              <span>
                directement au loueur — <span className="small muted">deux mouvements distincts ; le véhicule sort des disponibilités</span>
              </span>
            </label>
          </fieldset>
        )}
        <div className="form-grid">
          <OdometerFields last={vehicle.odometer_km} value={f.odometer} reason={f.reason} onValue={(v) => setF({ ...f, odometer: v })} onReason={(v) => setF({ ...f, reason: v })} />
          <Field label="Nombre de clés rendues">
            <input type="number" min={0} value={f.keys} onChange={(e) => setF({ ...f, keys: e.target.value })} />
          </Field>
          <Field label="Carburant">
            <input value={f.fuel} onChange={(e) => setF({ ...f, fuel: e.target.value })} />
          </Field>
          <Field label="Date et heure (Alger)" hint="Vide : maintenant">
            <input type="datetime-local" value={f.at} onChange={(e) => setF({ ...f, at: e.target.value })} />
          </Field>
        </div>
        <Field label="État au retour">
          <textarea rows={2} value={f.condition} onChange={(e) => setF({ ...f, condition: e.target.value })} />
        </Field>
        <Field label="Observations">
          <input value={f.note} onChange={(e) => setF({ ...f, note: e.target.value })} />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy}>
            Enregistrer la restitution
          </button>
        </div>
      </form>
    </Modal>
  );
}

function LessorReturnDialog({ vehicle, onClose, onDone }: { vehicle: any; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ odometer: "", reason: "", condition: "", note: "" });
  const cmd = useCommand();
  return (
    <Modal title={`Retour au loueur — ${vehicle.plate}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/vehicles/${vehicle.id}/lessor-return`, {
              body: { odometer_km: f.odometer ? Number(f.odometer) : null, odometer_reason: f.reason || null, condition: f.condition || null, note: f.note || null },
            }),
          );
          if (r) onDone();
        }}
      >
        <Notice kind="warn">Action définitive : le véhicule sort du parc disponible et le contrat est soldé.</Notice>
        <OdometerFields last={vehicle.odometer_km} value={f.odometer} reason={f.reason} onValue={(v) => setF({ ...f, odometer: v })} onReason={(v) => setF({ ...f, reason: v })} required={false} />
        <Field label="État remis au loueur">
          <input value={f.condition} onChange={(e) => setF({ ...f, condition: e.target.value })} />
        </Field>
        <Field label="Observations">
          <input value={f.note} onChange={(e) => setF({ ...f, note: e.target.value })} />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-danger" disabled={cmd.busy}>
            Confirmer le retour au loueur
          </button>
        </div>
      </form>
    </Modal>
  );
}

function DocumentDialog({ vehicle, onClose, onDone }: { vehicle: any; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ kind: "assurance", reference: "", valid_from: "", valid_until: "", note: "" });
  const cmd = useCommand();
  return (
    <Modal title={`Pièce du véhicule ${vehicle.plate}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/vehicles/${vehicle.id}/documents`, {
              body: { kind: f.kind, reference: f.reference || null, valid_from: f.valid_from || null, valid_until: f.valid_until || null, note: f.note || null },
            }),
          );
          if (r) onDone();
        }}
      >
        <p className="small muted">Un renouvellement s'ajoute : la pièce précédente reste dans l'historique.</p>
        <div className="form-grid">
          <Field label="Nature" required>
            <select value={f.kind} onChange={(e) => setF({ ...f, kind: e.target.value })}>
              {Object.entries(vehicleDocLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Référence (police, PV…)">
            <input value={f.reference} onChange={(e) => setF({ ...f, reference: e.target.value })} />
          </Field>
          <Field label="Valable du">
            <input type="date" value={f.valid_from} onChange={(e) => setF({ ...f, valid_from: e.target.value })} />
          </Field>
          <Field label="Valable jusqu'au">
            <input type="date" value={f.valid_until} onChange={(e) => setF({ ...f, valid_until: e.target.value })} />
          </Field>
        </div>
        <Field label="Note">
          <input value={f.note} onChange={(e) => setF({ ...f, note: e.target.value })} />
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

function MaintenanceDialog({ vehicle, onClose, onDone }: { vehicle: any; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ kind: "vidange", done_on: todayIso(), odometer: "", reason: "", description: "", provider: "", next_due_date: "", next_due_km: "" });
  const cmd = useCommand();
  return (
    <Modal title={`Intervention — ${vehicle.plate}`} onClose={onClose} wide>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/vehicles/${vehicle.id}/maintenance`, {
              body: {
                kind: f.kind,
                done_on: f.done_on,
                odometer_km: f.odometer ? Number(f.odometer) : null,
                odometer_reason: f.reason || null,
                description: f.description || null,
                provider: f.provider || null,
                next_due_date: f.next_due_date || null,
                next_due_km: f.next_due_km ? Number(f.next_due_km) : null,
              },
            }),
          );
          if (r) onDone();
        }}
      >
        <div className="form-grid">
          <Field label="Nature" required>
            <select value={f.kind} onChange={(e) => setF({ ...f, kind: e.target.value })}>
              {Object.entries(maintenanceLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Réalisée le" required>
            <input type="date" max={todayIso()} value={f.done_on} onChange={(e) => setF({ ...f, done_on: e.target.value })} required />
          </Field>
          <OdometerFields last={vehicle.odometer_km} value={f.odometer} reason={f.reason} onValue={(v) => setF({ ...f, odometer: v })} onReason={(v) => setF({ ...f, reason: v })} required={false} />
          <Field label="Prestataire">
            <input value={f.provider} onChange={(e) => setF({ ...f, provider: e.target.value })} />
          </Field>
        </div>
        <Field label="Description">
          <input value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
        </Field>
        <fieldset>
          <legend>Prochaine échéance (d'après la pièce réelle)</legend>
          <div className="form-grid">
            <Field label="Date">
              <input type="date" value={f.next_due_date} onChange={(e) => setF({ ...f, next_due_date: e.target.value })} />
            </Field>
            <Field label="Kilométrage">
              <input type="number" min={0} value={f.next_due_km} onChange={(e) => setF({ ...f, next_due_km: e.target.value })} />
            </Field>
          </div>
        </fieldset>
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

function LeaseDialog({ vehicle, renew, onClose, onDone }: { vehicle: any; renew: boolean; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ lessor: "", contract_no: "", start_date: "", end_date: "" });
  const cmd = useCommand();
  return (
    <Modal title={renew ? "Renouveler le contrat" : "Contrat de location"} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/vehicles/${vehicle.id}/leases`, {
              body: { lessor: f.lessor, contract_no: f.contract_no || null, start_date: f.start_date || null, end_date: f.end_date || null },
            }),
          );
          if (r) onDone();
        }}
      >
        {renew && <p className="small muted">Le contrat en cours est conservé dans l'historique comme « renouvelé ».</p>}
        <div className="form-grid">
          <Field label="Loueur" required>
            <input value={f.lessor} onChange={(e) => setF({ ...f, lessor: e.target.value })} required />
          </Field>
          <Field label="N° de contrat">
            <input value={f.contract_no} onChange={(e) => setF({ ...f, contract_no: e.target.value })} />
          </Field>
          <Field label="Début">
            <input type="date" value={f.start_date} onChange={(e) => setF({ ...f, start_date: e.target.value })} />
          </Field>
          <Field label="Fin">
            <input type="date" value={f.end_date} onChange={(e) => setF({ ...f, end_date: e.target.value })} />
          </Field>
        </div>
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

function StatusDialog({ vehicle, onClose, onDone }: { vehicle: any; onClose: () => void; onDone: () => void }) {
  const [status, setStatus] = useState(vehicle.status);
  const [reason, setReason] = useState("");
  const cmd = useCommand();
  return (
    <Modal title={`État du véhicule ${vehicle.plate}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() => api(`/vehicles/${vehicle.id}`, { method: "PATCH", body: { version: vehicle.version, status, reason } }));
          if (r) onDone();
        }}
      >
        <Field label="Nouvel état" required>
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="disponible">Disponible</option>
            <option value="immobilise">Immobilisé (garage, panne, papiers)</option>
            <option value="reforme">Réformé</option>
          </select>
        </Field>
        <Field label="Motif" required>
          <input value={reason} onChange={(e) => setReason(e.target.value)} required />
        </Field>
        <ErrorBox error={cmd.error} />
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary" disabled={cmd.busy || status === vehicle.status}>
            Enregistrer
          </button>
        </div>
      </form>
    </Modal>
  );
}
