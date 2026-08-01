import {
  CircleAlert,
  FilePenLine,
  RefreshCw,
  WifiOff,
} from "lucide-react";
import type { ConnectionState } from "../data/projectionState";
import type { ProjectMapProjection } from "../data/types";

interface ProjectionNoticeProps {
  projection: ProjectMapProjection | null;
  connection: ConnectionState;
  error: string;
}

export function ProjectionNotice({
  projection,
  connection,
  error,
}: ProjectionNoticeProps) {
  let icon = null;
  let message = "";
  let tone = "";

  if (projection?.health.status === "degraded" || error) {
    icon = <CircleAlert size={14} />;
    message =
      projection?.health.message ||
      (projection
        ? `${error} The last accepted map remains visible.`
        : error);
    tone = "is-warning";
  } else if (connection === "reconnecting" || connection === "offline") {
    icon = connection === "reconnecting" ? <RefreshCw size={14} /> : <WifiOff size={14} />;
    message =
      connection === "reconnecting"
        ? "Reconnecting to accepted state…"
        : "Live updates are unavailable. Manual refresh remains available.";
    tone = "is-muted";
  } else if (projection?.tree_editing) {
    icon = <FilePenLine size={14} />;
    message = "Tree Editing is open. Showing the last accepted topology.";
    tone = "is-editing";
  }

  if (!message) {
    return null;
  }
  return (
    <div className={`projection-notice ${tone}`} role="status">
      {icon}
      <span>{message}</span>
    </div>
  );
}
