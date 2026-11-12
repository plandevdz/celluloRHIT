import { useRef, useState } from "react";
import { ApiError, upload } from "../api";
import { Badge, ErrorBox } from "../components";
import { documentStatusLabel } from "../labels";

/** Imprimer la décharge ≠ la signer : « signé » exige le dépôt du scan. */
export function DocumentActions({ documentId, status, onChange }: { documentId: string; status: string; onChange: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const send = async (file: File) => {
    setBusy(true);
    setError(null);
    try {
      await upload(`/documents/${documentId}/scan`, file);
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
          {status === "signe" ? "Déposer un autre scan" : "Déposer le scan signé"}
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
      <ErrorBox error={error} />
    </div>
  );
}
