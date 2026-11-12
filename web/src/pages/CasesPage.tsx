import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api } from "../api";
import { ErrorBox, Loading, PageHeader, Pager, Section } from "../components";
import { useLoad } from "../hooks";
import { caseKindLabel, caseStatusLabel } from "../labels";
import { CaseTable } from "./HomePage";

export default function CasesPage() {
  const [params, setParams] = useSearchParams();
  const kind = params.get("type") ?? "";
  const status = params.get("statut") ?? "ouvert";
  const [q, setQ] = useState(params.get("q") ?? "");
  const page = Number(params.get("page") ?? "1");
  const query = new URLSearchParams({ page: String(page) });
  if (kind) query.set("kind", kind);
  if (status) query.set("status", status);
  if (params.get("q")) query.set("q", params.get("q")!);
  const cases = useLoad(() => api<{ items: any[] }>(`/cases?${query}`), [kind, status, params.get("q"), page]);

  const setFilter = (k: string, v: string) => {
    const p = new URLSearchParams(params);
    if (v) p.set(k, v);
    else p.delete(k);
    p.delete("page");
    setParams(p, { replace: true });
  };

  return (
    <>
      <PageHeader title="Dossiers" subtitle="Recherche par référence, nom ou matricule." />
      <form
        className="filters"
        onSubmit={(e) => {
          e.preventDefault();
          setFilter("q", q.trim());
        }}
      >
        <label className="field">
          <span className="field-label">Recherche</span>
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="ONB-00007, nom, matricule" />
        </label>
        <label className="field">
          <span className="field-label">Type</span>
          <select value={kind} onChange={(e) => setFilter("type", e.target.value)}>
            <option value="">Tous</option>
            {Object.entries(caseKindLabel).map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span className="field-label">Statut</span>
          <select value={status} onChange={(e) => setFilter("statut", e.target.value)}>
            <option value="">Tous</option>
            {Object.entries(caseStatusLabel).map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
        </label>
        <button className="btn" type="submit">
          Rechercher
        </button>
      </form>
      <ErrorBox error={cases.error} onRetry={cases.reload} />
      <Section title="Résultats">
        {cases.loading && !cases.data ? <Loading /> : <CaseTable items={cases.data?.items ?? []} empty="Aucun dossier." />}
        <Pager
          page={page}
          hasNext={(cases.data?.items.length ?? 0) === 50}
          onPage={(p) => {
            const n = new URLSearchParams(params);
            n.set("page", String(p));
            setParams(n);
          }}
        />
      </Section>
    </>
  );
}
