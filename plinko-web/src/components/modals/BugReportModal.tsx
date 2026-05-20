import { useState } from "react";
import { Modal } from "../Modal";
import type { PlanRequest, PlanResponse } from "../../protocol";

interface Props {
  sendRequest: (req: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function BugReportModal({ sendRequest, onClose }: Props) {
  const [description, setDescription] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!description.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const resp = await sendRequest({
        SubmitBugReport: {
          description: description.trim(),
          page_url: window.location.href,
          user_agent: navigator.userAgent,
        },
      });
      if (resp === "PlanUpdated") {
        setSubmitted(true);
      } else {
        setError("Failed to submit bug report.");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      title="Submit Bug Report"
      onClose={onClose}
      onSave={submitted ? undefined : handleSubmit}
    >
      {submitted ? (
        <div style={{ padding: "8px 0", color: "#81c784" }}>
          Thank you! Your bug report has been submitted.
        </div>
      ) : (
        <>
          <p style={{ margin: "0 0 12px 0", fontSize: 13, color: "#aaa" }}>
            Describe the issue you encountered. Browser and page information will be attached automatically.
          </p>
          <textarea
            style={{
              width: "100%",
              minHeight: 120,
              resize: "vertical",
              background: "#1e1e1e",
              border: "1px solid #3a3a3c",
              borderRadius: 4,
              color: "#d4d4d4",
              fontSize: 13,
              padding: "8px 10px",
              outline: "none",
              fontFamily: "inherit",
              boxSizing: "border-box",
            }}
            placeholder="Describe the bug…"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            autoFocus
          />
          {error && <div style={{ color: "#e57373", fontSize: 12, marginTop: 6 }}>{error}</div>}
          <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 16 }}>
            <button
              className="btn btn-primary"
              onClick={handleSubmit}
              disabled={submitting || !description.trim()}
            >
              {submitting ? "Submitting…" : "Submit"}
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
