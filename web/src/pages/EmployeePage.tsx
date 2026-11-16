import { FormEvent, useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { useAuth } from "../auth";
import { Badge, EmployeePicker, Empty, ErrorBox, Field, LegacyProof, Loading, Modal, Notice, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import {
  caseKindLabel,
  caseStatusLabel,
  contractTypeLabel,
  DEPARTMENTS,
  displayStatus,
  employeeStatusLabel,
  fmtDate,
  fmtDateTime,
  NEED_CATEGORIES,
  needLabel,
  needTeam,
  roleLabel,
  sessionStatusLabel,
} from "../labels";
import { ConfirmArrivalDialog } from "./TaskDialogs";
import { DepartureDialog, PositionCorrectionDialog, TransferDialog } from "./TransitionPanels";

export default function EmployeePage() {
  const { id = "" } = useParams();
  const { has, on } = useAuth();
  const location = useLocation();
  const detail = useLoad(() => api<any>(`/employees/${id}`), [id]);
  const accounts = useLoad(() => (has("IT", "RH", "ADMIN") ? api<any[]>(`/employees/${id}/accounts`) : Promise.resolve([])), [id]);
  const [dialog, setDialog] = useState<null | "request" | "edit" | "arrival" | "org" | "mutation" | "depart" | "correction">(null);

  useEffect(() => {
    if (location.hash === "#comptes" && accounts.data) document.getElementById("comptes")?.scrollIntoView();
  }, [location.hash, accounts.data]);

  if (detail.loading && !detail.data) return <Loading />;
  if (detail.error) return <ErrorBox error={detail.error} onRetry={detail.reload} />;
  const { employee: e, positions, cases, holdings, trainings, contact, scope, licenses = [], vehicles = [] } = detail.data;
  const openMut = cases.find((c: any) => c.kind === "MUT" && c.status === "ouvert");
  const openOff = cases.find((c: any) => c.kind === "OFF" && c.status === "ouvert");
  const full = scope === "complet";
  const isIt = has("IT", "ADMIN");
  const isHr = has("RH", "ADMIN");

  return (
    <>
      <PageHeader
        title={
          <span dir="auto">
            {e.first_name} {e.last_name}
          </span>
        }
        subtitle={
          <>
            <Badge status={displayStatus(e)} label={employeeStatusLabel[displayStatus(e)]} /> · {e.job_title ?? "poste ?"} · {e.site_name ?? "site ?"}
            {full && (e.matricule ? ` · matricule ${e.matricule}` : " · sans matricule")}
          </>
        }
        actions={
          <>
            {isHr && e.status === "a_venir" && (
              <button className="btn btn-primary" onClick={() => setDialog("arrival")}>
                Confirmer l'arrivée
              </button>
            )}
            {isHr && e.status === "actif" && e.actual_arrival && (
              <button className="btn" onClick={() => setDialog("arrival")}>
                Corriger la date d'arrivée
              </button>
            )}
            {isHr && e.status !== "sorti" && (
              <button className="btn btn-primary" onClick={() => setDialog("request")}>
                Nouvelle demande
              </button>
            )}
            {isHr && on("mutation") && e.status !== "sorti" && !openMut && !openOff && (
              <button className="btn" onClick={() => setDialog("mutation")}>
                Mutation
              </button>
            )}
            {isHr && on("depart") && e.status !== "sorti" && !openOff && (
              <button className="btn" onClick={() => setDialog("depart")}>
                Départ
              </button>
            )}
            {isHr && (
              <button className="btn" onClick={() => setDialog("edit")}>
                Modifier l'identité
              </button>
            )}
            {isHr && (
              <button className="btn" onClick={() => setDialog("org")}>
                Informations RH
              </button>
            )}
            {isIt && e.status !== "sorti" && (
              <Link className="btn" to={`/parc/remise?salarie=${e.id}`}>
                Remettre du matériel
              </Link>
            )}
            {isIt && holdings.length > 0 && (
              <Link className="btn" to={`/parc/restitution?salarie=${e.id}`}>
                Restitution
              </Link>
            )}
          </>
        }
      />
      <div className="grid-2">
        <div className="stack">
          <Section title="Situation">
            <dl className="kv">
              <dt>Poste</dt>
              <dd>{e.job_title ?? "—"}</dd>
              <dt>Site</dt>
              <dd>{e.site_name ?? "—"}</dd>
              <dt>Département</dt>
              <dd>{e.department ?? "—"}</dd>
              <dt>Manager</dt>
              <dd>
                {e.manager_employee_id ? (
                  <Link to={`/personnel/${e.manager_employee_id}`}>{e.manager_employee_name}</Link>
                ) : e.manager ? (
                  <>
                    {e.manager} <span className="tag">à rattacher à une fiche</span>
                  </>
                ) : (
                  "—"
                )}
              </dd>
              {full && (
                <>
                  <dt>Contrat</dt>
                  <dd>{e.contract_type ? contractTypeLabel[e.contract_type] : "—"}</dd>
                  <dt>Profil de poste</dt>
                  <dd>{e.job_profile_name ?? "—"}</dd>
                  {e.poste_fixe && (
                    <>
                      <dt>« Poste fixe » (ancien champ)</dt>
                      <dd>
                        {e.poste_fixe} <span className="small muted">sens à confirmer</span>
                      </dd>
                    </>
                  )}
                  {e.leaving && (
                    <>
                      <dt>En départ</dt>
                      <dd>sortie prévue le {fmtDate(e.planned_exit_date)}</dd>
                    </>
                  )}
                  {e.import_note && (
                    <>
                      <dt>Remarque de reprise</dt>
                      <dd className="small">{e.import_note}</dd>
                    </>
                  )}
                  <dt>Wilayas</dt>
                  <dd>{e.wilayas?.length ? e.wilayas.map((w: any) => `${w.code} ${w.name}`).join(", ") : "—"}</dd>
                </>
              )}
              {full && (
                <>
                  <dt>Arrivée</dt>
                  <dd>
                    {e.actual_arrival ? `${fmtDate(e.actual_arrival)} (présence confirmée)` : `${fmtDate(e.planned_arrival)} (prévue, présence non confirmée)`}
                    {e.arrival_confirmed_by_name && (
                      <div className="small muted">
                        par {e.arrival_confirmed_by_name} le {fmtDateTime(e.arrival_confirmed_at)}
                      </div>
                    )}
                  </dd>
                </>
              )}
              {contact && (
                <>
                  <dt>Email pro</dt>
                  <dd>{contact.email_pro ?? "non consigné"}</dd>
                  <dt>Lignes mobiles</dt>
                  <dd>
                    {(contact.lines ?? []).length === 0
                      ? contact.mobile ?? "aucune ligne remise"
                      : contact.lines.map((l: any) => (
                          <div key={l.line_id}>
                            {l.number} <span className="small muted">{l.operator}</span>{" "}
                            {l.primary ? (
                              <span className="tag">principale</span>
                            ) : (
                              (isHr || isIt) &&
                              full && (
                                <button
                                  className="btn-link small"
                                  onClick={async () => {
                                    await api(`/employees/${e.id}/primary-line`, { body: { line_id: l.line_id } });
                                    detail.reload();
                                  }}
                                >
                                  définir comme principale
                                </button>
                              )
                            )}
                          </div>
                        ))}
                  </dd>
                </>
              )}
              {e.exit_date && (
                <>
                  <dt>Sortie</dt>
                  <dd>{fmtDate(e.exit_date)}</dd>
                </>
              )}
            </dl>
            {openMut && (
              <Notice kind="info">
                Mutation en cours : <Link to={`/dossiers/${openMut.reference}`}>{openMut.reference}</Link> (la situation ci-dessus reste celle en vigueur).
              </Notice>
            )}
            {openOff && (
              <Notice kind="warn">
                Départ en cours : <Link to={`/dossiers/${openOff.reference}`}>{openOff.reference}</Link>
              </Notice>
            )}
            {isHr && on("mutation") && e.status !== "sorti" && (
              <button className="btn-link small" onClick={() => setDialog("correction")}>
                Correction de poste datée…
              </button>
            )}
            {positions.length > 1 && (
              <details>
                <summary>Historique du poste</summary>
                <ul className="timeline">
                  {positions.map((p: any, i: number) => (
                    <li key={i}>
                      {p.job_title} · {p.site} · du {fmtDate(p.valid_from)} {p.valid_to ? `au ${fmtDate(p.valid_to)}` : "à aujourd'hui"}
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </Section>

          {full && (
          <Section title="Biens détenus">
            {holdings.length === 0 ? (
              <Empty>Aucun bien détenu.</Empty>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>Élément</th>
                    <th>Identifiant</th>
                    <th className="num">Qté</th>
                    <th>Remise d'origine</th>
                  </tr>
                </thead>
                <tbody>
                  {holdings.map((h: any) => (
                    <tr key={h.assignment_id}>
                      <td>{h.label}</td>
                      <td className="mono small">{h.identifier ?? "—"}</td>
                      <td className="num">
                        {h.open_quantity}
                        {h.open_quantity !== h.quantity && <span className="muted">/{h.quantity}</span>}
                      </td>
                      <td className="small">
                        {h.origin_reference} · {fmtDateTime(h.since)}
                        {h.reprise_note && (
                          <>
                            <div className="muted">{h.reprise_note}</div>
                            <LegacyProof url={h.legacy_proof_url} reference={h.legacy_proof_ref} />
                          </>
                        )}
                        {h.case_reference && (
                          <>
                            {" "}
                            · <Link to={`/dossiers/${h.case_reference}`}>{h.case_reference}</Link>
                          </>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Section>
          )}

          {full && on("logiciels") && licenses.length > 0 && (
            <Section title="Licences logicielles">
              <ul className="timeline">
                {licenses.map((l: any) => (
                  <li key={l.allocation_id}>
                    <Link to={`/parc/logiciels/${l.subscription_id}`}>{l.label}</Link> · depuis {fmtDateTime(l.since)}
                    {l.end_date && <span className="small muted"> · échéance {fmtDate(l.end_date)}</span>}
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {isHr && on("vehicules") && vehicles.length > 0 && (
            <Section title="Véhicule confié">
              <ul className="timeline">
                {vehicles.map((v: any) => (
                  <li key={v.assignment_id}>
                    <Link to={`/vehicules/${v.vehicle_id}`} className="mono">
                      {v.plate}
                    </Link>{" "}
                    {v.label} · depuis {fmtDateTime(v.since)} ({v.handover_reference})
                  </li>
                ))}
              </ul>
            </Section>
          )}

          {(trainings.length > 0 || isHr) && (
          <Section title="Formations">
            {trainings.length === 0 ? (
              <Empty>Aucune formation.</Empty>
            ) : (
              <ul className="timeline">
                {trainings.map((t: any) => (
                  <li key={t.id}>
                    {t.name} · <Badge status={t.status} label={sessionStatusLabel[t.status]} />
                    {t.done_date && ` le ${fmtDate(t.done_date)}`}
                    {(t.trainer || t.recorded_by || t.reprise) && (
                      <div className="small muted">
                        {[t.trainer && `animée par ${t.trainer}`, t.recorded_by && `enregistrée par ${t.recorded_by}`, t.reprise && "reprise SharePoint"]
                          .filter(Boolean)
                          .join(" · ")}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Section>
          )}
        </div>

        <div className="stack">
          <Section title="Dossiers">
            {cases.length === 0 ? (
              <Empty>Aucun dossier.</Empty>
            ) : (
              <ul className="timeline">
                {cases.map((c: any) => (
                  <li key={c.id}>
                    <Link to={`/dossiers/${c.reference}`}>
                      <strong>{c.reference}</strong>
                    </Link>{" "}
                    — {caseKindLabel[c.kind]} · <Badge status={c.status} label={caseStatusLabel[c.status]} />{" "}
                    <span className="muted small">{fmtDateTime(c.created_at)}</span>
                  </li>
                ))}
              </ul>
            )}
          </Section>

          {has("IT", "RH", "ADMIN") && (
            <div id="comptes">
              <AccountsSection employee={e} accounts={accounts.data ?? []} canEdit={isIt} onChange={() => { accounts.reload(); detail.reload(); }} />
            </div>
          )}
        </div>
      </div>

      {dialog === "request" && <RequestDialog employee={e} cases={cases} onClose={() => setDialog(null)} />}
      {dialog === "arrival" && (
        <ConfirmArrivalDialog
          employee={e}
          onClose={() => setDialog(null)}
          onDone={() => {
            setDialog(null);
            detail.reload();
          }}
        />
      )}
      {dialog === "edit" && <EditIdentityDialog employee={e} onClose={() => setDialog(null)} onSaved={() => { setDialog(null); detail.reload(); }} />}
      {dialog === "org" && <EditOrgDialog employee={e} onClose={() => setDialog(null)} onSaved={() => { setDialog(null); detail.reload(); }} />}
      {dialog === "mutation" && <TransferDialog employee={e} onClose={() => setDialog(null)} />}
      {dialog === "depart" && <DepartureDialog employee={e} onClose={() => setDialog(null)} />}
      {dialog === "correction" && <PositionCorrectionDialog employee={e} onClose={() => setDialog(null)} onDone={() => { setDialog(null); detail.reload(); }} />}
    </>
  );
}

const systemLabel: Record<string, string> = { email: "Adresse email", xena: "Xena", intelix: "Intelix", autre: "Autre" };
const platformLabel: Record<string, string> = { cpanel: "cPanel", m365: "Microsoft 365" };
const accountStatusLabel: Record<string, string> = { actif: "Actif", a_verifier: "À vérifier", coupe: "Coupé" };

/** Comptes réellement ouverts par l'IT dans les systèmes externes. Aucun mot de passe. */
function AccountsSection({ employee, accounts, canEdit, onChange }: { employee: any; accounts: any[]; canEdit: boolean; onChange: () => void }) {
  const [form, setForm] = useState({ system: "email", platform: "", identifier: "" });
  const cmd = useCommand();
  const add = async (e: FormEvent) => {
    e.preventDefault();
    const r = await cmd.run(() => api(`/employees/${employee.id}/accounts`, { body: { ...form, platform: form.platform || null } }));
    if (r) {
      setForm({ system: "email", platform: "", identifier: "" });
      onChange();
    }
  };
  return (
    <Section title="Comptes et accès">
      <label className="inline-choice" style={{ marginBottom: ".75rem" }}>
        <input
          type="checkbox"
          disabled={!canEdit || cmd.busy}
          checked={!!employee.m365_verified_at}
          onChange={async (ev) => {
            await cmd.run(() => api(`/employees/${employee.id}/m365`, { body: { verified: ev.target.checked } }));
            onChange();
          }}
        />
        <strong>Compte Microsoft 365 opérationnel — vérifié</strong>
        {employee.m365_verified_at && <span className="small muted">({fmtDateTime(employee.m365_verified_at)})</span>}
      </label>
      <p className="small muted" style={{ marginTop: 0 }}>Indépendant de l'hébergement de l'adresse email.</p>
      {accounts.length === 0 ? (
        <Empty>Aucun compte consigné.</Empty>
      ) : (
        <table>
          <tbody>
            {accounts.map((a) => (
              <tr key={a.id}>
                <td>
                  {systemLabel[a.system]}
                  {a.platform && <div className="small muted">{platformLabel[a.platform]}</div>}
                </td>
                <td className="mono small">{a.identifier}</td>
                <td>
                  <Badge status={a.status === "coupe" ? "annule" : a.status === "a_verifier" ? "a_verifier" : "termine"} label={accountStatusLabel[a.status]} />
                  <div className="small muted">
                    {a.verified_at
                      ? `vérifié ${fmtDateTime(a.verified_at)}`
                      : a.imported
                        ? "Adresse reprise — vérification non renseignée"
                        : "vérification non renseignée"}
                  </div>
                </td>
                <td>
                  {canEdit && a.status !== "coupe" && (
                    <select
                      aria-label="Changer le statut"
                      value=""
                      onChange={async (ev) => {
                        if (!ev.target.value) return;
                        await cmd.run(() => api(`/employees/${employee.id}/accounts/${a.id}`, { method: "PATCH", body: { status: ev.target.value } }));
                        onChange();
                      }}
                    >
                      <option value="">Confirmer…</option>
                      {(a.status !== "actif" || !a.verified_at) && <option value="actif">Actif (vérifié)</option>}
                      {a.status !== "a_verifier" && <option value="a_verifier">À vérifier</option>}
                      <option value="coupe">Coupure réalisée</option>
                    </select>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {canEdit && (
        <form onSubmit={add} style={{ marginTop: ".75rem" }}>
          <div className="form-grid">
            <Field label="Système">
              <select value={form.system} onChange={(e) => setForm({ ...form, system: e.target.value, platform: "" })}>
                {Object.entries(systemLabel).map(([k, l]) => (
                  <option key={k} value={k}>
                    {l}
                  </option>
                ))}
              </select>
            </Field>
            {form.system === "email" && (
              <Field label="Plateforme effective" required>
                <select value={form.platform} onChange={(e) => setForm({ ...form, platform: e.target.value })} required>
                  <option value="">Choisir…</option>
                  <option value="cpanel">cPanel</option>
                  <option value="m365">Microsoft 365</option>
                </select>
              </Field>
            )}
            <Field label={form.system === "email" ? "Adresse réelle" : "Identifiant"} required>
              <input value={form.identifier} onChange={(e) => setForm({ ...form, identifier: e.target.value })} required />
            </Field>
          </div>
          <ErrorBox error={cmd.error} />
          <div className="form-actions" style={{ marginTop: 0 }}>
            <button className="btn btn-primary" type="submit" disabled={cmd.busy}>
              Consigner le compte créé
            </button>
          </div>
          <p className="small muted">Aucun mot de passe n'est saisi ni conservé.</p>
        </form>
      )}
    </Section>
  );
}

/** M04 : demande ponctuelle autonome, une seule équipe sollicitée. */
function RequestDialog({ employee, cases, onClose }: { employee: any; cases: any[]; onClose: () => void }) {
  const navigate = useNavigate();
  const { on } = useAuth();
  const catalog = useLoad(() => api<any[]>("/trainings/catalog"), []);
  const [f, setF] = useState({ category: "carte_visite", detail: "", due_date: "", parent_case_id: "", preference: "", catalog_id: "" });
  const [key] = useState(newKey);
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (force = false) => {
    setError(null);
    try {
      const r = await api<{ reference: string }>(`/employees/${employee.id}/requests`, {
        idempotencyKey: force ? `${key}-force` : key,
        body: {
          category: f.category,
          detail: f.detail || null,
          due_date: f.due_date || null,
          parent_case_id: f.parent_case_id || null,
          preference: f.category === "pc" ? f.preference || null : null,
          training: f.category === "formation" ? { catalog_id: f.catalog_id } : null,
          force,
        },
      });
      navigate(`/dossiers/${r.reference}`);
    } catch (e) {
      setError(e as ApiError);
    }
  };
  const existing: string | undefined = error?.detail?.existing_reference;
  return (
    <Modal title={`Nouvelle demande — ${employee.first_name} ${employee.last_name}`} onClose={onClose}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit(false);
        }}
      >
        <Field label="Besoin" required>
          <select value={f.category} onChange={(e) => setF({ ...f, category: e.target.value })}>
            {NEED_CATEGORIES.filter((c) => c !== "vehicule" || on("vehicules")).map((c) => (
              <option key={c} value={c}>
                {needLabel[c]} — {roleLabel[needTeam[c]]}
              </option>
            ))}
          </select>
        </Field>
        {f.category === "pc" && (
          <Field label="Préférence">
            <select value={f.preference} onChange={(e) => setF({ ...f, preference: e.target.value })}>
              <option value="">—</option>
              <option value="fixe">Fixe</option>
              <option value="portable">Portable</option>
              <option value="sans_preference">Sans préférence</option>
            </select>
          </Field>
        )}
        {f.category === "formation" && (
          <Field label="Formation" required>
            <select value={f.catalog_id} onChange={(e) => setF({ ...f, catalog_id: e.target.value })} required>
              <option value="">Choisir…</option>
              {catalog.data?.filter((t) => t.active).map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </Field>
        )}
        <Field label="Précision">
          <textarea rows={2} value={f.detail} onChange={(e) => setF({ ...f, detail: e.target.value })} />
        </Field>
        <div className="form-grid">
          <Field label="Échéance souhaitée">
            <input type="date" value={f.due_date} onChange={(e) => setF({ ...f, due_date: e.target.value })} />
          </Field>
          <Field label="Dossier d'origine (facultatif)">
            <select value={f.parent_case_id} onChange={(e) => setF({ ...f, parent_case_id: e.target.value })}>
              <option value="">Aucun</option>
              {cases.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.reference} ({caseStatusLabel[c.status]})
                </option>
              ))}
            </select>
          </Field>
        </div>
        <p className="small muted">Seule l'équipe {roleLabel[needTeam[f.category]]} est prévenue ; le dossier d'origine n'est pas rouvert.</p>
        {existing ? (
          <Notice kind="warn">
            {error?.message}.{" "}
            <Link to={`/dossiers/${existing}`} onClick={onClose}>
              Ouvrir {existing}
            </Link>{" "}
            ou{" "}
            <button type="button" className="btn-link" onClick={() => submit(true)}>
              créer quand même une nouvelle demande
            </button>
            .
          </Notice>
        ) : (
          <ErrorBox error={error} />
        )}
        <div className="form-actions">
          <button type="button" className="btn" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="btn btn-primary">
            Créer la demande
          </button>
        </div>
      </form>
    </Modal>
  );
}

function EditIdentityDialog({ employee, onClose, onSaved }: { employee: any; onClose: () => void; onSaved: () => void }) {
  const [f, setF] = useState({ first_name: employee.first_name, last_name: employee.last_name, matricule: employee.matricule ?? "" });
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api(`/employees/${employee.id}`, { method: "PATCH", body: { version: employee.version, ...f } });
      onSaved();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title="Modifier l'identité" onClose={onClose}>
      <form onSubmit={submit}>
        <Field label="Prénom" required>
          <input value={f.first_name} onChange={(e) => setF({ ...f, first_name: e.target.value })} required dir="auto" />
        </Field>
        <Field label="Nom" required>
          <input value={f.last_name} onChange={(e) => setF({ ...f, last_name: e.target.value })} required dir="auto" />
        </Field>
        <Field label="Matricule" hint="Vider le champ efface le matricule (action tracée).">
          <input value={f.matricule} onChange={(e) => setF({ ...f, matricule: e.target.value })} />
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

/** Données RH de la situation en cours : poste, site, département, contrat,
 *  poste fixe, wilayas, manager, profil. Une mutation (module MUT) reste le
 *  chemin d'un changement de poste daté et préparé. */
function EditOrgDialog({ employee, onClose, onSaved }: { employee: any; onClose: () => void; onSaved: () => void }) {
  const wilayas = useLoad(() => api<any[]>("/wilayas"), []);
  const profiles = useLoad(() => api<any[]>("/job-profiles"), []);
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({
    job_title: employee.job_title ?? "",
    site_id: employee.site_id ?? "",
    leaving: !!employee.leaving,
    planned_exit_date: employee.planned_exit_date ?? "",
    contract_type: employee.contract_type ?? "",
    poste_fixe: employee.poste_fixe ?? "",
    department: employee.department ?? "",
    job_profile_id: employee.job_profile_id ?? "",
  });
  const [selected, setSelected] = useState<string[]>((employee.wilayas ?? []).map((w: any) => w.id));
  const [manager, setManager] = useState<{ id: string; name: string } | null>(
    employee.manager_employee_id ? { id: employee.manager_employee_id, name: employee.manager_employee_name } : null,
  );
  const [error, setError] = useState<ApiError | null>(null);
  const submit = async (ev: FormEvent) => {
    ev.preventDefault();
    try {
      await api(`/employees/${employee.id}`, {
        method: "PATCH",
        body: {
          version: employee.version,
          job_title: f.job_title !== (employee.job_title ?? "") ? f.job_title : undefined,
          site_id: f.site_id && f.site_id !== employee.site_id ? f.site_id : undefined,
          contract_type: f.contract_type,
          poste_fixe: f.poste_fixe,
          leaving: f.leaving,
          planned_exit_date: f.leaving ? f.planned_exit_date : "",
          department: f.department,
          job_profile_id: f.job_profile_id || null,
          wilaya_ids: selected,
          manager_employee_id: manager?.id ?? null,
          clear_manager: !manager && !!employee.manager_employee_id,
        },
      });
      onSaved();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  return (
    <Modal title="Informations RH" onClose={onClose} wide>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="Type de contrat">
            <select value={f.contract_type} onChange={(e) => setF({ ...f, contract_type: e.target.value })}>
              <option value="">—</option>
              {Object.entries(contractTypeLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Poste" hint="Correction directe de la situation en cours (tracée dans l'historique de la fiche).">
            <input value={f.job_title} onChange={(e) => setF({ ...f, job_title: e.target.value })} required />
          </Field>
          <Field label="Site">
            <select value={f.site_id} onChange={(e) => setF({ ...f, site_id: e.target.value })} required>
              <option value="">—</option>
              {sites.data
                ?.filter((s) => s.active || s.id === employee.site_id)
                .map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
            </select>
          </Field>
          <Field label="« Poste fixe » (ancien champ, sens à confirmer)">
            <input value={f.poste_fixe} onChange={(e) => setF({ ...f, poste_fixe: e.target.value })} />
          </Field>
          <Field label="Département">
            <input list="departements-org" value={f.department} onChange={(e) => setF({ ...f, department: e.target.value })} />
            <datalist id="departements-org">
              {DEPARTMENTS.map((d) => (
                <option key={d} value={d} />
              ))}
            </datalist>
          </Field>
          <Field label="Profil de poste">
            <select value={f.job_profile_id} onChange={(e) => setF({ ...f, job_profile_id: e.target.value })}>
              <option value="">—</option>
              {profiles.data?.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <EmployeePicker value={manager} onChange={setManager} label="Manager (facultatif)" />
        {employee.status !== "sorti" && (
          <fieldset>
            <legend>En départ</legend>
            <label className="inline-choice">
              <input type="checkbox" checked={f.leaving} onChange={(e) => setF({ ...f, leaving: e.target.checked })} /> La personne prépare sa sortie
              (elle reste dans l'effectif ; aucun processus de départ n'est lancé)
            </label>
            {f.leaving && (
              <Field label="Date de sortie prévue">
                <input type="date" value={f.planned_exit_date} onChange={(e) => setF({ ...f, planned_exit_date: e.target.value })} />
              </Field>
            )}
          </fieldset>
        )}
        <fieldset>
          <legend>Wilayas couvertes</legend>
          {wilayas.data?.length === 0 && <p className="small muted">Aucune wilaya au référentiel (Administration → Référentiels).</p>}
          <div className="inline-choice" style={{ maxHeight: 180, overflowY: "auto" }}>
            {wilayas.data?.filter((w) => w.active).map((w) => (
              <label key={w.id}>
                <input type="checkbox" checked={selected.includes(w.id)} onChange={(e) => setSelected(e.target.checked ? [...selected, w.id] : selected.filter((x) => x !== w.id))} />
                {w.code} {w.name}
              </label>
            ))}
          </div>
        </fieldset>
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
