import { X } from "lucide-react";
import type { BranchDetailState } from "../data/useBranchDetail";
import type {
  ProjectMapDependency,
  ProjectMapNode,
} from "../data/types";
import {
  statusPresentation,
  verificationPresentation,
} from "../app/status";

interface InspectorProps {
  node: ProjectMapNode | null;
  detailState: BranchDetailState;
  dependencies: ProjectMapDependency[];
  nodes: ProjectMapNode[];
  annotation: string;
  onAnnotationChange: (value: string) => void;
  onSelectRelated: (id: string) => void;
  onClose: () => void;
}

function NarrativeSection({
  title,
  value,
}: {
  title: string;
  value: string;
}) {
  if (!value.trim()) {
    return null;
  }
  return (
    <section className="inspector-section">
      <h3>{title}</h3>
      <div className="narrative-text">{value}</div>
    </section>
  );
}

export function Inspector({
  node,
  detailState,
  dependencies,
  nodes,
  annotation,
  onAnnotationChange,
  onSelectRelated,
  onClose,
}: InspectorProps) {
  if (!node) {
    return null;
  }
  const status = statusPresentation(node);
  const verification = verificationPresentation(node.verification);
  const detail = detailState.detail;
  const byId = new Map(nodes.map((item) => [item.id, item]));
  const upstream = dependencies
    .filter((dependency) => dependency.from === node.id)
    .map((dependency) => ({
      dependency,
      node: byId.get(dependency.to),
    }))
    .filter((item): item is typeof item & { node: ProjectMapNode } =>
      Boolean(item.node),
    );
  const downstream = dependencies
    .filter((dependency) => dependency.to === node.id)
    .map((dependency) => ({
      dependency,
      node: byId.get(dependency.from),
    }))
    .filter((item): item is typeof item & { node: ProjectMapNode } =>
      Boolean(item.node),
    );

  return (
    <aside
      id="branchInspector"
      className="branch-inspector"
      aria-label={`Branch details for ${node.title}`}
    >
      <header className="inspector-header">
        <div>
          <span className={`inspector-status ${status.className}`}>
            <span aria-hidden="true">{status.symbol}</span>
            {status.label}
          </span>
          <h2>{node.title}</h2>
          <code>{node.id}</code>
        </div>
        <button
          type="button"
          className="icon-button"
          aria-label="Close branch inspector"
          onClick={onClose}
        >
          <X size={17} />
        </button>
      </header>

      <div className="inspector-scroll">
        {(detailState.phase === "loading" ||
          detailState.phase === "stale" ||
          detailState.phase === "unavailable") && (
          <div
            className={`inspector-fetch-state ${detailState.phase}`}
            role="status"
          >
            {detailState.phase === "loading" && "Loading branch narrative…"}
            {detailState.phase === "stale" &&
              `Refreshing narrative. ${detailState.error}`.trim()}
            {detailState.phase === "unavailable" &&
              (detailState.error || "Branch narrative is unavailable.")}
          </div>
        )}

        <section className="inspector-overview">
          <p>{node.purpose}</p>
          <dl>
            <div>
              <dt>Verification</dt>
              <dd className={verification.className}>
                <span aria-hidden="true">{verification.symbol}</span>{" "}
                {verification.label}
              </dd>
            </div>
            {node.status_reason && (
              <div>
                <dt>Status reason</dt>
                <dd>{node.status_reason}</dd>
              </div>
            )}
            {node.parent && (
              <div>
                <dt>Parent</dt>
                <dd>
                  <button
                    type="button"
                    className="related-link"
                    onClick={() => onSelectRelated(node.parent)}
                  >
                    {byId.get(node.parent)?.title ?? node.parent}
                  </button>
                </dd>
              </div>
            )}
            {node.spec && (
              <div>
                <dt>Spec reference</dt>
                <dd>
                  <code>{node.spec}</code>
                </dd>
              </div>
            )}
          </dl>
        </section>

        {(upstream.length > 0 || downstream.length > 0) && (
          <section className="inspector-section">
            <h3>Dependencies</h3>
            <div className="related-list">
              {upstream.map(({ dependency, node: related }) => (
                <button
                  type="button"
                  key={`up:${related.id}`}
                  onClick={() => onSelectRelated(related.id)}
                >
                  <span>{dependency.satisfied ? "✓" : "○"} prerequisite</span>
                  <strong>{related.title}</strong>
                </button>
              ))}
              {downstream.map(({ node: related }) => (
                <button
                  type="button"
                  key={`down:${related.id}`}
                  onClick={() => onSelectRelated(related.id)}
                >
                  <span>→ unlocks</span>
                  <strong>{related.title}</strong>
                </button>
              ))}
            </div>
          </section>
        )}

        {detail && (
          <>
            <NarrativeSection title="Scope" value={detail.task_plan.scope} />
            <NarrativeSection
              title="Acceptance"
              value={detail.task_plan.acceptance}
            />
            <NarrativeSection
              title="Current reality"
              value={detail.progress.current_reality}
            />
            <NarrativeSection
              title="Recent work"
              value={detail.progress.recent_work}
            />
            <NarrativeSection
              title="Open issues"
              value={detail.progress.open_issues}
            />
            <NarrativeSection
              title="Decisions"
              value={detail.findings.decisions}
            />
            <NarrativeSection
              title="Contract effects"
              value={detail.findings.interface_or_contract_effects}
            />
            <NarrativeSection
              title="Risks and unknowns"
              value={detail.findings.risks_and_unknowns}
            />
            <NarrativeSection
              title="Verification evidence"
              value={detail.verification.evidence}
            />
            <NarrativeSection
              title="Coverage gap"
              value={detail.verification.coverage_gap}
            />
          </>
        )}

        <section className="inspector-section annotation-section">
          <label htmlFor="sessionAnnotation">Session annotation</label>
          <textarea
            id="sessionAnnotation"
            className="session-annotation"
            value={annotation}
            rows={4}
            placeholder="Temporary thought for this browser session"
            onChange={(event) => onAnnotationChange(event.currentTarget.value)}
          />
          <small>Local to this browser session. TreeWork state is unchanged.</small>
        </section>
      </div>
    </aside>
  );
}
