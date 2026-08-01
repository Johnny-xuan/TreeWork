import { useEffect, useState } from "react";
import {
  readSession,
  writeSession,
  type ProjectMapSession,
} from "./session";

export function useProjectMapSession() {
  const [session, setSession] = useState<ProjectMapSession>(readSession);

  useEffect(() => {
    const timeout = window.setTimeout(() => writeSession(session), 120);
    return () => window.clearTimeout(timeout);
  }, [session]);

  return [session, setSession] as const;
}
