import { api } from "../api";
import { useLoad } from "../hooks";
import { answerLabel, NEED_CATEGORIES, needLabel, needTeam, roleLabel } from "../labels";

export type NeedValue = {
  category: string;
  answer: "oui" | "non" | "a_preciser";
  preference?: string | null;
  quantity?: number | null;
  detail?: string | null;
};

export type TrainingValue = { catalog_id: string; trainer_id?: string | null };

export function emptyNeeds(): Record<string, NeedValue> {
  const r: Record<string, NeedValue> = {};
  for (const c of NEED_CATEGORIES) r[c] = { category: c, answer: "a_preciser" };
  return r;
}

/** Saisie Oui / Non / À préciser par besoin ; PC : préférence distincte. */
export function NeedsEditor({
  needs,
  onChange,
  trainings,
  onTrainings,
  existingTrainingIds = [],
}: {
  needs: Record<string, NeedValue>;
  onChange: (n: Record<string, NeedValue>) => void;
  trainings: TrainingValue[];
  onTrainings: (t: TrainingValue[]) => void;
  existingTrainingIds?: string[];
}) {
  const catalog = useLoad(() => api<{ id: string; name: string; active: boolean }[]>("/trainings/catalog"), []);
  const trainers = useLoad(() => api<{ id: string; display_name: string }[]>("/users/directory?role=FORMATION"), []);
  const set = (c: string, patch: Partial<NeedValue>) => onChange({ ...needs, [c]: { ...needs[c], ...patch } });

  return (
    <div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Besoin</th>
              <th>Équipe</th>
              <th>Réponse</th>
              <th>Précision</th>
            </tr>
          </thead>
          <tbody>
            {NEED_CATEGORIES.filter((c) => c !== "formation").map((c) => {
              const n = needs[c];
              return (
                <tr key={c}>
                  <td>
                    <strong>{needLabel[c]}</strong>
                  </td>
                  <td className="small muted">{roleLabel[needTeam[c]]}</td>
                  <td>
                    <div className="inline-choice" role="radiogroup" aria-label={needLabel[c]}>
                      {(["oui", "non", "a_preciser"] as const).map((a) => (
                        <label key={a}>
                          <input type="radio" name={`need-${c}`} checked={n.answer === a} onChange={() => set(c, { answer: a })} />
                          {answerLabel[a]}
                        </label>
                      ))}
                    </div>
                  </td>
                  <td>
                    <div className="row">
                      {c === "pc" && (
                        <select
                          aria-label="Préférence de PC"
                          value={n.preference ?? ""}
                          onChange={(e) => set(c, { preference: e.target.value || null })}
                        >
                          <option value="">Préférence…</option>
                          <option value="fixe">Fixe</option>
                          <option value="portable">Portable</option>
                          <option value="sans_preference">Sans préférence</option>
                        </select>
                      )}
                      <input
                        aria-label={`Précision ${needLabel[c]}`}
                        placeholder="Précision facultative"
                        value={n.detail ?? ""}
                        onChange={(e) => set(c, { detail: e.target.value })}
                        style={{ flex: 1, minWidth: 160 }}
                      />
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="small muted">
        L'IT choisit la plateforme email (cPanel ou Microsoft 365) et peut remplacer le PC préféré selon le stock, en le
        justifiant. Une réponse non renseignée reste « à préciser ».
      </p>

      <fieldset>
        <legend>Formations à planifier</legend>
        {catalog.data?.filter((t) => t.active).length === 0 && <p className="muted small">Catalogue vide.</p>}
        {catalog.data
          ?.filter((t) => t.active)
          .map((t) => {
            const selected = trainings.find((x) => x.catalog_id === t.id);
            const existing = existingTrainingIds.includes(t.id);
            return (
              <div key={t.id} className="row" style={{ marginBottom: ".4rem" }}>
                <label className="inline-choice">
                  <input
                    type="checkbox"
                    disabled={existing}
                    checked={existing || !!selected}
                    onChange={(e) =>
                      onTrainings(
                        e.target.checked ? [...trainings, { catalog_id: t.id }] : trainings.filter((x) => x.catalog_id !== t.id),
                      )
                    }
                  />
                  {t.name}
                  {existing && <span className="small muted">(déjà prévue)</span>}
                </label>
                {selected && (
                  <select
                    aria-label={`Intervenant pour ${t.name}`}
                    value={selected.trainer_id ?? ""}
                    onChange={(e) =>
                      onTrainings(trainings.map((x) => (x.catalog_id === t.id ? { ...x, trainer_id: e.target.value || null } : x)))
                    }
                  >
                    <option value="">File de l'équipe Formation</option>
                    {trainers.data?.map((u) => (
                      <option key={u.id} value={u.id}>
                        {u.display_name}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            );
          })}
      </fieldset>
    </div>
  );
}

/** Codes des équipes qui verront l'annonce. */
export function recipients(needs: Record<string, NeedValue>, trainings: TrainingValue[]): string[] {
  const teams = new Set<string>(["IT", "FORMATION"]);
  for (const n of Object.values(needs)) if (n.answer === "oui") teams.add(needTeam[n.category]);
  if (trainings.length) teams.add("FORMATION");
  return [...teams];
}

export function needsPayload(needs: Record<string, NeedValue>): NeedValue[] {
  return Object.values(needs).map((n) => ({
    category: n.category,
    answer: n.answer,
    preference: n.category === "pc" ? n.preference || null : null,
    quantity: n.quantity || null,
    detail: n.detail?.trim() || null,
  }));
}
