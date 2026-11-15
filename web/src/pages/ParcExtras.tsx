import { FormEvent, useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api, ApiError, newKey } from "../api";
import { Badge, Empty, EmployeePicker, ErrorBox, Field, Loading, Modal, Notice, PageHeader, Section } from "../components";
import { useCommand, useLoad } from "../hooks";
import {
  fmtDate,
  fmtDateTime,
  licenseKindLabel,
  lineStatusLabel,
  lineTypeLabel,
  scanResultLabel,
  seatReleaseLabel,
} from "../labels";

// ------------------------------------------------------------------ étiquettes

const FORMATS = [
  { id: "a4-24", label: "Planche A4 — 24 étiquettes 70 × 37 mm", per: 24 },
  { id: "a4-65", label: "Planche A4 — 65 étiquettes 38,1 × 21,2 mm", per: 65 },
  { id: "unitaire", label: "Étiquette unitaire 62 × 29 mm (imprimante d'étiquettes)", per: 1 },
];

/** Impression à l'échelle réelle (100 %, sans « ajuster à la page »). */
export function LabelsDialog({ assets, onClose }: { assets: { id: string; inventory_no: string }[]; onClose: () => void }) {
  const [format, setFormat] = useState("a4-24");
  const [start, setStart] = useState(1);
  const per = FORMATS.find((f) => f.id === format)!.per;
  const url = `/api/v1/assets/labels.pdf?ids=${assets.map((a) => a.id).join(",")}&format=${format}&start=${start}`;
  return (
    <Modal title={`Étiquettes QR (${assets.length})`} onClose={onClose}>
      <Field label="Format">
        <select value={format} onChange={(e) => setFormat(e.target.value)}>
          {FORMATS.map((f) => (
            <option key={f.id} value={f.id}>
              {f.label}
            </option>
          ))}
        </select>
      </Field>
      {per > 1 && (
        <Field label="Position de la première étiquette libre" hint="Pour réutiliser une planche entamée (1 = en haut à gauche).">
          <input type="number" min={1} max={per} value={start} onChange={(e) => setStart(Math.min(per, Math.max(1, Number(e.target.value) || 1)))} />
        </Field>
      )}
      <p className="small muted">
        {assets.slice(0, 6).map((a) => a.inventory_no).join(", ")}
        {assets.length > 6 && ` et ${assets.length - 6} autre(s)`}. Le QR contient seulement le jeton permanent de l'actif ; une réimpression donne
        le même code. Imprimez à 100 % et testez la lecture d'une planche avant un grand tirage.
      </p>
      <div className="form-actions">
        <button type="button" className="btn" onClick={onClose}>
          Fermer
        </button>
        <a className="btn btn-primary" href={url} target="_blank" rel="noopener">
          Ouvrir l'aperçu PDF
        </a>
      </div>
    </Modal>
  );
}

// ------------------------------------------------------------------ logiciels

export function SoftwareTab() {
  const list = useLoad(() => api<any[]>("/software"), []);
  const [editing, setEditing] = useState<any | null>(null);
  return (
    <Section title="Logiciels et abonnements" actions={<button className="btn btn-primary" onClick={() => setEditing({})}>+ Abonnement ou licence</button>}>
      <p className="small muted">Les sièges ne reçoivent pas d'étiquette QR. Une licence perpétuelle n'a pas de date de renouvellement.</p>
      <ErrorBox error={list.error} onRetry={list.reload} />
      {list.loading && !list.data ? (
        <Loading />
      ) : list.data?.length === 0 ? (
        <Empty>Aucun logiciel suivi.</Empty>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Logiciel</th>
              <th>Type</th>
              <th className="num">Sièges</th>
              <th>Échéance</th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((s) => (
              <tr key={s.id}>
                <td>
                  <Link to={`/parc/logiciels/${s.id}`}>
                    <strong>
                      {s.vendor} — {s.product}
                    </strong>
                  </Link>
                  {s.offer && <div className="small muted">{s.offer}</div>}
                  {!s.active && <span className="tag">Inactif</span>}
                </td>
                <td className="small">{licenseKindLabel[s.license_kind]}</td>
                <td className="num">
                  <strong>{s.seats - s.used}</strong> libre(s) / {s.seats}
                  <div className="small muted">{s.holders} titulaire(s)</div>
                </td>
                <td className="small">
                  {s.license_kind === "perpetuelle" ? "—" : fmtDate(s.end_date)}
                  {s.auto_renew && <div className="muted">renouvellement automatique</div>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {editing && <SubscriptionDialog sub={editing.id ? editing : null} onClose={() => setEditing(null)} onDone={() => { setEditing(null); list.reload(); }} />}
    </Section>
  );
}

function SubscriptionDialog({ sub, onClose, onDone }: { sub: any | null; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({
    vendor: sub?.vendor ?? "",
    product: sub?.product ?? "",
    offer: sub?.offer ?? "",
    contract_ref: sub?.contract_ref ?? "",
    license_kind: sub?.license_kind ?? "abonnement",
    start_date: sub?.start_date ?? "",
    end_date: sub?.end_date ?? "",
    auto_renew: !!sub?.auto_renew,
    seats: String(sub?.seats ?? 1),
    seat_release: sub?.seat_release ?? "immediat",
    notes: sub?.notes ?? "",
  });
  const [error, setError] = useState<ApiError | null>(null);
  const perpetual = f.license_kind === "perpetuelle";
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      const body = {
        ...f,
        version: sub?.version,
        offer: f.offer || null,
        contract_ref: f.contract_ref || null,
        start_date: f.start_date || null,
        end_date: perpetual ? null : f.end_date || null,
        auto_renew: perpetual ? false : f.auto_renew,
        seats: Number(f.seats),
        notes: f.notes || null,
      };
      await api(sub ? `/software/${sub.id}` : "/software", { method: sub ? "PATCH" : "POST", body });
      onDone();
    } catch (err) {
      setError(err as ApiError);
    }
  };
  const bind = (k: keyof typeof f) => ({ value: f[k] as string, onChange: (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value }) });
  return (
    <Modal title={sub ? "Modifier l'abonnement" : "Nouvel abonnement ou licence"} onClose={onClose} wide>
      <form onSubmit={submit}>
        <div className="form-grid">
          <Field label="Éditeur" required>
            <input {...bind("vendor")} required />
          </Field>
          <Field label="Produit" required>
            <input {...bind("product")} required />
          </Field>
          <Field label="Offre">
            <input {...bind("offer")} />
          </Field>
          <Field label="Référence du contrat">
            <input {...bind("contract_ref")} />
          </Field>
          <Field label="Type" required>
            <select {...bind("license_kind")}>
              <option value="abonnement">Abonnement</option>
              <option value="perpetuelle">Licence perpétuelle</option>
            </select>
          </Field>
          <Field label="Sièges achetés" required>
            <input type="number" min={0} {...bind("seats")} required />
          </Field>
          <Field label="Début">
            <input type="date" {...bind("start_date")} />
          </Field>
          {!perpetual && (
            <Field label="Échéance">
              <input type="date" {...bind("end_date")} />
            </Field>
          )}
          <Field label="Siège rendu">
            <select {...bind("seat_release")}>
              {Object.entries(seatReleaseLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
        </div>
        {!perpetual && (
          <label className="inline-choice">
            <input type="checkbox" checked={f.auto_renew} onChange={(e) => setF({ ...f, auto_renew: e.target.checked })} /> Renouvellement automatique prévu au contrat
          </label>
        )}
        <Field label="Notes">
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

export function SoftwarePage() {
  const { id = "" } = useParams();
  const d = useLoad(() => api<any>(`/software/${id}`), [id]);
  const [editing, setEditing] = useState(false);
  const [emp, setEmp] = useState<{ id: string; name: string } | null>(null);
  const [key, setKey] = useState(newKey);
  const cmd = useCommand();
  if (d.loading && !d.data) return <Loading />;
  if (d.error) return <ErrorBox error={d.error} onRetry={d.reload} />;
  const { subscription: s, allocations } = d.data;
  const free = s.seats - s.used;
  const allocate = async (e: FormEvent) => {
    e.preventDefault();
    if (!emp) return;
    const r = await cmd.run(() => api(`/software/${s.id}/allocations`, { idempotencyKey: key, body: { employee_id: emp.id } }));
    if (r) {
      setEmp(null);
      setKey(newKey());
      d.reload();
    }
  };
  return (
    <>
      <PageHeader
        title={`${s.vendor} — ${s.product}`}
        subtitle={
          <>
            {licenseKindLabel[s.license_kind]} · {free} siège(s) libre(s) sur {s.seats}
            {s.license_kind === "abonnement" && ` · échéance ${fmtDate(s.end_date)}`}
          </>
        }
        actions={
          <>
            <button className="btn" onClick={() => setEditing(true)}>
              Modifier / renouveler
            </button>
            <Link className="btn" to="/parc?onglet=logiciels">
              Retour
            </Link>
          </>
        }
      />
      <div className="grid-2">
        <Section title="Attribuer un siège">
          {free <= 0 ? (
            <Notice kind="warn">Plus de siège disponible : libérez un siège ou augmentez la capacité achetée.</Notice>
          ) : (
            <form onSubmit={allocate}>
              <EmployeePicker value={emp} onChange={setEmp} required />
              <ErrorBox error={cmd.error} />
              <button className="btn btn-primary" disabled={!emp || cmd.busy}>
                Attribuer
              </button>
            </form>
          )}
          <p className="small muted">{seatReleaseLabel[s.seat_release]} après restitution.</p>
        </Section>
        <Section title="Titulaires">
          {allocations.length === 0 ? (
            <Empty>Aucun siège attribué.</Empty>
          ) : (
            <table>
              <tbody>
                {allocations.map((a: any) => (
                  <tr key={a.id} className={a.ended_at ? "muted" : ""}>
                    <td>
                      <Link to={`/personnel/${a.employee_id}`}>{a.employee}</Link>
                      <div className="small muted">depuis {fmtDateTime(a.started_at)}</div>
                    </td>
                    <td className="small">
                      {a.ended_at ? (
                        <>
                          Libéré le {fmtDateTime(a.ended_at)}
                          {a.seat_free_at ? ` · siège disponible le ${fmtDateTime(a.seat_free_at)}` : " · siège non réattribuable"}
                        </>
                      ) : (
                        <Badge status="affecte" label="Attribué" />
                      )}
                    </td>
                    <td>
                      {!a.ended_at && (
                        <button
                          className="btn btn-small"
                          onClick={async () => {
                            await cmd.run(() => api(`/license-allocations/${a.id}/release`, { body: {} }));
                            d.reload();
                          }}
                        >
                          Libérer
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Section>
      </div>
      {editing && <SubscriptionDialog sub={s} onClose={() => setEditing(false)} onDone={() => { setEditing(false); d.reload(); }} />}
    </>
  );
}

// ------------------------------------------------------------------ lignes

export function LinesSection() {
  const list = useLoad(() => api<any[]>("/lines"), []);
  const [editing, setEditing] = useState<any | null>(null);
  return (
    <Section title="Lignes mobiles">
      {list.data?.length === 0 ? (
        <Empty>Aucune ligne.</Empty>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Numéro</th>
              <th>Forfait</th>
              <th>État</th>
              <th>SIM · titulaire</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {list.data?.map((l) => (
              <tr key={l.id}>
                <td>
                  <strong>{l.number}</strong>
                  <div className="small muted">{l.operator}</div>
                </td>
                <td className="small">
                  {l.plan ?? "—"}
                  {l.line_type && <div className="muted">{lineTypeLabel[l.line_type]}</div>}
                </td>
                <td>
                  <Badge status={l.status} label={lineStatusLabel[l.status]} />
                </td>
                <td className="small">
                  <span className="mono">{l.iccid ?? "—"}</span>
                  <div>{l.holder ?? "non attribuée"}</div>
                </td>
                <td>
                  <button className="btn btn-small" onClick={() => setEditing(l)}>
                    Modifier
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {editing && <LineDialog line={editing} onClose={() => setEditing(null)} onDone={() => { setEditing(null); list.reload(); }} />}
    </Section>
  );
}

function LineDialog({ line, onClose, onDone }: { line: any; onClose: () => void; onDone: () => void }) {
  const [f, setF] = useState({ plan: line.plan ?? "", line_type: line.line_type ?? "", status: line.status, reason: "" });
  const cmd = useCommand();
  return (
    <Modal title={`Ligne ${line.number}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/lines/${line.id}`, { method: "PATCH", body: { plan: f.plan || null, line_type: f.line_type || null, status: f.status, reason: f.reason || null } }),
          );
          if (r) onDone();
        }}
      >
        <div className="form-grid">
          <Field label="Forfait">
            <input value={f.plan} onChange={(e) => setF({ ...f, plan: e.target.value })} />
          </Field>
          <Field label="Type">
            <select value={f.line_type} onChange={(e) => setF({ ...f, line_type: e.target.value })}>
              <option value="">—</option>
              {Object.entries(lineTypeLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field label="État">
            <select value={f.status} onChange={(e) => setF({ ...f, status: e.target.value })}>
              {Object.entries(lineStatusLabel).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
        </div>
        {f.status !== "active" && f.status !== line.status && (
          <Field label="Motif" required>
            <input value={f.reason} onChange={(e) => setF({ ...f, reason: e.target.value })} required />
          </Field>
        )}
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

// ------------------------------------------------------------------ inventaires

export function InventoriesTab() {
  const list = useLoad(() => api<any[]>("/inventories"), []);
  const sites = useLoad(() => api<any[]>("/sites"), []);
  const [f, setF] = useState({ name: "", site_id: "", location_id: "" });
  const cmd = useCommand();
  const navigate = useNavigate();
  const site = sites.data?.find((s) => s.id === f.site_id);
  return (
    <div className="grid-2">
      <Section title="Campagnes d'inventaire">
        {list.data?.length === 0 ? (
          <Empty>Aucune campagne.</Empty>
        ) : (
          <table>
            <tbody>
              {list.data?.map((c) => (
                <tr key={c.id}>
                  <td>
                    <Link to={`/parc/inventaires/${c.id}`}>
                      <strong>{c.reference}</strong>
                    </Link>{" "}
                    {c.name}
                    <div className="small muted">
                      {c.site}
                      {c.location && ` · ${c.location}`} · référence {fmtDateTime(c.reference_at)}
                    </div>
                  </td>
                  <td>
                    <Badge status={c.status} label={c.status === "ouverte" ? "Ouverte" : "Close"} />
                  </td>
                  <td className="num small">
                    {c.seen}/{c.expected} vus
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>
      <Section title="Nouvelle campagne">
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            const r = await cmd.run(() => api<any>("/inventories", { body: { name: f.name, site_id: f.site_id, location_id: f.location_id || null } }));
            if (r) navigate(`/parc/inventaires/${r.id}`);
          }}
        >
          <Field label="Nom" required>
            <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} required placeholder="Inventaire annuel 2026" />
          </Field>
          <div className="form-grid">
            <Field label="Site" required>
              <select value={f.site_id} onChange={(e) => setF({ ...f, site_id: e.target.value, location_id: "" })} required>
                <option value="">Choisir…</option>
                {sites.data?.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Local (facultatif)" hint="Vide : tout le site">
              <select value={f.location_id} onChange={(e) => setF({ ...f, location_id: e.target.value })} disabled={!site}>
                <option value="">Tout le site</option>
                {site?.locations.map((l: any) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </select>
            </Field>
          </div>
          <p className="small muted">Le périmètre attendu et l'heure de référence sont figés à l'ouverture.</p>
          <ErrorBox error={cmd.error} />
          <button className="btn btn-primary" disabled={cmd.busy}>
            Ouvrir la campagne
          </button>
        </form>
      </Section>
    </div>
  );
}

const resultTone: Record<string, string> = { vu: "ok", ailleurs: "warn", hors_perimetre: "warn", inconnu: "danger", doublon: "muted" };

/** Poste de scan : la zone garde le focus (douchette en mode clavier). */
export function InventoryPage() {
  const { id = "" } = useParams();
  const d = useLoad(() => api<any>(`/inventories/${id}`), [id]);
  const report = useLoad(() => api<any>(`/inventories/${id}/report`), [id]);
  const [loc, setLoc] = useState(() => {
    try {
      return window.sessionStorage.getItem(`inventaire-local-${id}`) ?? "";
    } catch {
      return "";
    }
  });
  const [code, setCode] = useState("");
  const [manual, setManual] = useState(false);
  const [last, setLast] = useState<any | null>(null);
  const [counters, setCounters] = useState<any | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [arbitrating, setArbitrating] = useState<any | null>(null);
  const [closing, setClosing] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
  }, [loc, last]);
  if (d.loading && !d.data) return <Loading />;
  if (d.error) return <ErrorBox error={d.error} onRetry={d.reload} />;
  const c = d.data;
  const open = c.status === "ouverte";
  const k = counters ?? c.counters;

  const scan = async (e: FormEvent) => {
    e.preventDefault();
    const value = code.trim();
    if (!value || !loc) return;
    setCode("");
    setError(null);
    try {
      const r = await api<any>(`/inventories/${id}/scan`, { body: { code: value, location_id: loc, command_key: newKey(), manual } });
      setLast({ ...r, code: value });
      setCounters(r.counters);
      d.reload();
      report.reload();
    } catch (err) {
      setError(err as ApiError);
    }
  };

  return (
    <>
      <PageHeader
        title={`${c.reference} — ${c.name}`}
        subtitle={
          <>
            {c.site}
            {c.location && ` · ${c.location}`} · référence {fmtDateTime(c.reference_at)} ·{" "}
            <Badge status={c.status} label={open ? "Ouverte" : "Close"} />
          </>
        }
        actions={
          <>
            <a className="btn" href={`/api/v1/inventories/${id}/report.xlsx`}>
              Exporter le rapport (Excel)
            </a>
            {open && (
              <button className="btn" onClick={() => setClosing(true)}>
                Clore la campagne
              </button>
            )}
          </>
        }
      />
      <div className="stat-row">
        <div className="stat">
          <div className="value">{k.expected}</div>
          <div className="label">Attendus</div>
        </div>
        <div className="stat">
          <div className="value">{k.seen_ok}</div>
          <div className="label">Vus au bon endroit</div>
        </div>
        <div className="stat">
          <div className="value">{k.seen_elsewhere}</div>
          <div className="label">Vus ailleurs</div>
        </div>
        <div className="stat">
          <div className="value">{k.not_seen}</div>
          <div className="label">Non vus (à vérifier)</div>
        </div>
        <div className="stat">
          <div className="value">{k.unknown + k.out_of_scope}</div>
          <div className="label">Inconnus · hors périmètre</div>
        </div>
      </div>
      {open && (
        <div className="scan-station">
          <div className="form-grid">
            <Field label="Local où je scanne" required>
              <select
                value={loc}
                onChange={(e) => {
                  setLoc(e.target.value);
                  try {
                    window.sessionStorage.setItem(`inventaire-local-${id}`, e.target.value);
                  } catch {
                    /* stockage indisponible */
                  }
                }}
              >
                <option value="">Choisir le local…</option>
                {c.locations.map((l: any) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </select>
            </Field>
            <label className="inline-choice" style={{ alignSelf: "end", marginBottom: ".9rem" }}>
              <input type="checkbox" checked={manual} onChange={(e) => setManual(e.target.checked)} /> Saisie manuelle (étiquette illisible)
            </label>
          </div>
          <form onSubmit={scan}>
            <input
              ref={input}
              className="scan-input"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder={loc ? "Scanner le QR ou saisir le n° d'inventaire, puis Entrée" : "Choisissez d'abord le local"}
              disabled={!loc}
              aria-label="Code scanné"
              autoComplete="off"
            />
          </form>
          {last && (
            <div className={`scan-result ${resultTone[last.result] ?? "muted"}`} role="status">
              {last.message}
              <div className="small" style={{ fontWeight: 400 }}>
                {last.asset ? `${last.asset.inventory_no} — ${last.asset.label}` : last.code} · {scanResultLabel[last.result]}
                {last.replayed && " (déjà enregistré)"}
              </div>
            </div>
          )}
          <ErrorBox error={error} />
          <p className="small muted">Un pointage ne change ni la garde ni l'emplacement d'un actif. Un code inconnu n'ouvre aucun lien.</p>
        </div>
      )}
      <div className="grid-2" style={{ marginTop: "1rem" }}>
        <Section title="Écarts" actions={<button className="btn btn-small" onClick={report.reload}>Actualiser</button>}>
          {!report.data ? (
            <Loading />
          ) : (
            <>
              <Gaps title="Vus ailleurs" items={report.data.ailleurs} render={(x) => `${x.inventory_no} — attendu ${x.expected_location ?? "?"}, vu ${x.observed_location}`} onArbitrate={setArbitrating} />
              <Gaps title="Non vus (à vérifier, pas perdus)" items={report.data.non_vus} render={(x) => `${x.inventory_no} — ${x.label}${x.holder ? ` · détenu par ${x.holder}` : ""}`} onArbitrate={setArbitrating} />
              <Gaps title="Hors périmètre" items={report.data.hors_perimetre} render={(x) => `${x.inventory_no} — parc : ${x.site ?? "sans site"}, vu ${x.observed_location}`} onArbitrate={setArbitrating} />
              <Gaps title="Plusieurs locaux (conflit)" items={report.data.plusieurs_locaux} render={(x) => `${x.inventory_no} — ${x.locations.join(", ")}`} onArbitrate={setArbitrating} />
              <Gaps title="Codes inconnus" items={report.data.inconnus} render={(x) => `${x.code} — ${x.location}`} onArbitrate={(x) => setArbitrating({ ...x, code: x.code })} />
              <Gaps title="Mouvements depuis la référence" items={report.data.mouvements_depuis_reference} render={(x) => `${x.inventory_no} — ${x.movement} (${fmtDateTime(x.at)})`} />
              {report.data.arbitrages.length > 0 && (
                <details>
                  <summary>Arbitrages ({report.data.arbitrages.length})</summary>
                  <ul className="timeline">
                    {report.data.arbitrages.map((a: any, i: number) => (
                      <li key={i}>
                        {a.decision.replace("_", " ")} — {a.note} <span className="muted">({a.by}, {fmtDateTime(a.at)})</span>
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
        </Section>
        <Section title="Derniers pointages">
          {c.recent.length === 0 ? (
            <Empty>Aucun pointage.</Empty>
          ) : (
            <ul className="timeline">
              {c.recent.map((r: any, i: number) => (
                <li key={i}>
                  <Badge status={r.result} label={scanResultLabel[r.result]} /> {r.inventory_no ?? r.scanned_code} · {r.location}{" "}
                  <span className="small muted">
                    {fmtDateTime(r.observed_at)} · {r.operator}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Section>
      </div>
      {arbitrating && (
        <ArbitrationDialog
          campaign={id}
          item={arbitrating}
          onClose={() => setArbitrating(null)}
          onDone={() => {
            setArbitrating(null);
            report.reload();
          }}
        />
      )}
      {closing && <CloseInventoryDialog id={id} onClose={() => setClosing(false)} onDone={() => { setClosing(false); d.reload(); }} />}
    </>
  );
}

function Gaps({ title, items, render, onArbitrate }: { title: string; items: any[]; render: (x: any) => string; onArbitrate?: (x: any) => void }) {
  if (!items?.length) return null;
  return (
    <details open={items.length <= 10}>
      <summary>
        {title} ({items.length})
      </summary>
      <ul className="timeline">
        {items.map((x, i) => (
          <li key={i}>
            {x.asset_id ? <Link to={`/parc/actifs/${x.asset_id}`}>{render(x)}</Link> : render(x)}{" "}
            {onArbitrate && (
              <button className="btn-link small" onClick={() => onArbitrate(x)}>
                arbitrer
              </button>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}

function ArbitrationDialog({ campaign, item, onClose, onDone }: { campaign: string; item: any; onClose: () => void; onDone: () => void }) {
  const [decision, setDecision] = useState("a_rechercher");
  const [note, setNote] = useState("");
  const cmd = useCommand();
  return (
    <Modal title={`Arbitrer — ${item.inventory_no ?? item.code}`} onClose={onClose}>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          const r = await cmd.run(() =>
            api(`/inventories/${campaign}/arbitrations`, { body: { asset_id: item.asset_id ?? null, observation_code: item.code ?? null, decision, note } }),
          );
          if (r) onDone();
        }}
      >
        <Field label="Décision">
          <select value={decision} onChange={(e) => setDecision(e.target.value)}>
            <option value="a_rechercher">À rechercher</option>
            <option value="explique">Écart expliqué</option>
            <option value="parc_rectifie">Parc rectifié (après rectification explicite de l'actif)</option>
          </select>
        </Field>
        <Field label="Motif" required>
          <textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} required />
        </Field>
        {item.asset_id && (
          <p className="small">
            Pour corriger l'emplacement du parc : <Link to={`/parc/actifs/${item.asset_id}`}>ouvrir l'actif</Link> puis « rectifier ».
          </p>
        )}
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

function CloseInventoryDialog({ id, onClose, onDone }: { id: string; onClose: () => void; onDone: () => void }) {
  const [note, setNote] = useState("");
  const cmd = useCommand();
  return (
    <Modal title="Clore la campagne" onClose={onClose}>
      <p className="small">Plus aucun pointage ne sera accepté. Le rapport reste consultable et exportable.</p>
      <Field label="Note de clôture">
        <textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} />
      </Field>
      <ErrorBox error={cmd.error} />
      <div className="form-actions">
        <button className="btn" onClick={onClose}>
          Annuler
        </button>
        <button
          className="btn btn-primary"
          disabled={cmd.busy}
          onClick={async () => {
            const r = await cmd.run(() => api(`/inventories/${id}/close`, { body: { note: note || null } }));
            if (r) onDone();
          }}
        >
          Clore
        </button>
      </div>
    </Modal>
  );
}
