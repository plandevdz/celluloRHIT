import { useEffect, useRef, useState } from "react";
import { ApiError, upload } from "../api";
import { Badge, ErrorBox } from "../components";
import { documentStatusLabel } from "../labels";

/** Imprimer la décharge ≠ la signer : « signé » exige le dépôt du scan. */
export function DocumentActions({ documentId, status: initial, onChange }: { documentId: string; status: string; onChange: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  // L'état renvoyé par le serveur fait foi tout de suite : l'IT n'a pas à
  // recharger la page (ni à redéposer) pour savoir si le scan est enregistré.
  const [status, setStatus] = useState(initial);
  const [saved, setSaved] = useState<{ version: number; name: string } | null>(null);
  useEffect(() => setStatus(initial), [initial]);
  const send = async (file: File) => {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      const r = await upload<{ status: string; version: number }>(`/documents/${documentId}/scan`, file);
      setStatus(r.status);
      setSaved({ version: r.version, name: file.name });
      onChange();
    } catch (e) {
      setError(e as ApiError);
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };
  return (
    <div>
      <div className="row">
        <Badge status={status} label={documentStatusLabel[status] ?? status} />
        <a className="btn btn-small" href={`/api/v1/documents/${documentId}/original`} target="_blank" rel="noopener">
          Imprimer la décharge (PDF)
        </a>
        <button className="btn btn-small" disabled={busy} onClick={() => input.current?.click()}>
          {busy ? "Envoi du scan…" : status === "signe" ? "Déposer un autre scan" : "Déposer le scan signé"}
        </button>
        <input
          ref={input}
          type="file"
          accept="application/pdf,image/png,image/jpeg"
          hidden
          onChange={(e) => e.target.files?.[0] && send(e.target.files[0])}
        />
      </div>
      <p className="small muted" style={{ margin: ".3rem 0 0" }}>
        Archivage SharePoint : non raccordé dans cette version (copie conservée sur le serveur).
      </p>
      {saved && (
        <p className="alert alert-ok" role="status" style={{ margin: ".4rem 0 0" }}>
          Scan enregistré (version {saved.version}, {saved.name}) : décharge signée.
        </p>
      )}
      <ErrorBox error={error} />
    </div>
  );
}
