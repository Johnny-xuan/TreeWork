import type {
  ReplayBranchOption,
} from "../../data/replayState";
import type {
  ProjectMapNode,
  ReplayTransaction,
} from "../../data/types";
import {
  formatReplayChanges,
  formatReplayTime,
  replayEventLabel,
} from "./changes";

interface ReplayTransactionDetailProps {
  transaction: ReplayTransaction | null;
  branchOptions: readonly ReplayBranchOption[];
  knownNodes: readonly ProjectMapNode[];
}

export function ReplayTransactionDetail({
  transaction,
  branchOptions,
  knownNodes,
}: ReplayTransactionDetailProps) {
  const labels = new Map(
    branchOptions.map((option) => [option.id, option.label]),
  );
  knownNodes.forEach((node) => {
    labels.set(node.id, `${node.title} · ${node.id}`);
  });
  const branchLabel = (id: string) => labels.get(id) ?? (id || "none");

  if (!transaction) {
    return (
      <aside
        className="replay-transaction-detail is-empty"
        aria-label="Replay transaction detail"
      >
        <strong>No transaction selected</strong>
      </aside>
    );
  }

  const changes = formatReplayChanges(transaction, branchLabel);
  return (
    <aside
      className="replay-transaction-detail"
      aria-label={`Transaction ${transaction.seq} detail`}
    >
      <header>
        <div>
          <span>Accepted transaction</span>
          <h2>{replayEventLabel(transaction.type)}</h2>
        </div>
        <strong className={transaction.replayable ? "" : "is-unavailable"}>
          Seq {transaction.seq}
        </strong>
      </header>

      <div className="replay-detail-scroll">
        <p className="replay-transaction-message">{transaction.message}</p>
        <dl>
          <div>
            <dt>Time</dt>
            <dd>
              <time dateTime={transaction.time}>
                {formatReplayTime(transaction.time)}
              </time>
            </dd>
          </div>
          <div>
            <dt>Event type</dt>
            <dd><code>{transaction.type}</code></dd>
          </div>
          <div>
            <dt>Subject</dt>
            <dd><code>{branchLabel(transaction.subject)}</code></dd>
          </div>
          <div>
            <dt>Tree revision</dt>
            <dd>{transaction.tree_revision ?? "Not recorded"}</dd>
          </div>
          <div>
            <dt>Affected branches</dt>
            <dd>
              {transaction.affected_subjects.length
                ? transaction.affected_subjects
                    .map(branchLabel)
                    .join(" · ")
                : "None recorded"}
            </dd>
          </div>
          <div>
            <dt>Replayability</dt>
            <dd className={transaction.replayable ? "" : "is-unavailable"}>
              {transaction.replayable
                ? "Replayable"
                : transaction.replayability_reason ||
                  "Typed replay data is unavailable"}
            </dd>
          </div>
        </dl>

        <section>
          <h3>Semantic changes</h3>
          <ul>
            {changes.map((change, index) => (
              <li key={`${transaction.seq}:${index}`}>{change}</li>
            ))}
          </ul>
        </section>
      </div>
    </aside>
  );
}
