import {
  ChevronLeft,
  ChevronRight,
  Pause,
  Play,
  RotateCcw,
  X,
} from "lucide-react";
import type {
  ReplayMeta,
  ReplaySpeed,
  ReplayTransaction,
} from "../../data/types";
import {
  REPLAY_SPEEDS,
  type ReplayBranchOption,
} from "../../data/replayState";
import { formatReplayTime } from "./changes";

interface ReplayTimelineProps {
  transactions: readonly ReplayTransaction[];
  selectedIndex: number;
  selectedSequence: number | null;
  selectedTransaction: ReplayTransaction | null;
  meta: ReplayMeta;
  branchOptions: readonly ReplayBranchOption[];
  branchFilter: string;
  speed: ReplaySpeed;
  playing: boolean;
  atLive: boolean;
  liveAdvanced: boolean;
  refreshing: boolean;
  onSelectIndex: (index: number) => void;
  onStep: (offset: number) => void;
  onTogglePlaying: () => void;
  onBranchFilterChange: (branch: string) => void;
  onSpeedChange: (speed: ReplaySpeed) => void;
  onReturnToLive: () => void;
}

export function ReplayTimeline({
  transactions,
  selectedIndex,
  selectedSequence,
  selectedTransaction,
  meta,
  branchOptions,
  branchFilter,
  speed,
  playing,
  atLive,
  liveAdvanced,
  refreshing,
  onSelectIndex,
  onStep,
  onTogglePlaying,
  onBranchFilterChange,
  onSpeedChange,
  onReturnToLive,
}: ReplayTimelineProps) {
  const hasPrevious = selectedIndex > 0;
  const hasNext =
    selectedIndex >= 0 && selectedIndex < transactions.length - 1;
  const positionText =
    selectedIndex >= 0
      ? `${selectedIndex + 1} of ${transactions.length}`
      : `0 of ${transactions.length}`;
  const sequence =
    selectedTransaction?.seq ?? selectedSequence ?? meta.at_event_seq;

  return (
    <section className="replay-timeline" aria-label="Replay timeline">
      <div className="replay-transport">
        <button
          type="button"
          className="icon-button"
          aria-label={playing ? "Pause Replay" : "Play Replay"}
          title={playing ? "Pause Replay (Space)" : "Play Replay (Space)"}
          disabled={!playing && !hasNext}
          onClick={onTogglePlaying}
        >
          {playing ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Previous transaction"
          title="Previous transaction (Left arrow)"
          disabled={!hasPrevious}
          onClick={() => onStep(-1)}
        >
          <ChevronLeft size={17} />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Next transaction"
          title="Next transaction (Right arrow)"
          disabled={!hasNext}
          onClick={() => onStep(1)}
        >
          <ChevronRight size={17} />
        </button>

        <div className="replay-position" aria-live="polite">
          <strong>Seq {sequence}</strong>
          <span>{positionText}</span>
          {selectedTransaction && (
            <time dateTime={selectedTransaction.time}>
              {formatReplayTime(selectedTransaction.time)}
            </time>
          )}
        </div>

        <div className="replay-live-position">
          <span className={atLive ? "is-live" : ""}>
            {atLive ? "Live" : `Live seq ${meta.live_event_seq}`}
          </span>
          {refreshing && <span>Updating</span>}
          {liveAdvanced && (
            <button type="button" onClick={onReturnToLive}>
              <RotateCcw size={14} />
              Return to Live
            </button>
          )}
        </div>
      </div>

      <div className="replay-scrubber">
        <input
          type="range"
          min="0"
          max={Math.max(0, transactions.length - 1)}
          step="1"
          value={Math.max(0, selectedIndex)}
          disabled={!transactions.length}
          aria-label="Replay transaction"
          aria-valuetext={`Transaction ${positionText}, sequence ${sequence}`}
          onChange={(event) =>
            onSelectIndex(Number(event.currentTarget.value))
          }
        />
        <div aria-hidden="true">
          <span>{transactions[0]?.seq ?? "-"}</span>
          <span>{transactions[transactions.length - 1]?.seq ?? "-"}</span>
        </div>
      </div>

      {branchFilter && transactions.length === 0 && (
        <p className="replay-empty-filter" role="status">
          No accepted transactions mention this branch.
        </p>
      )}

      <div className="replay-options">
        <label className="replay-branch-filter">
          <span>Branch history</span>
          <select
            aria-label="Filter Replay by branch"
            value={branchFilter}
            onChange={(event) =>
              onBranchFilterChange(event.currentTarget.value)
            }
          >
            <option value="">All branches</option>
            {branchOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="icon-button replay-clear-filter"
          aria-label="Clear Replay branch filter"
          title="Clear branch filter"
          disabled={!branchFilter}
          onClick={() => onBranchFilterChange("")}
        >
          <X size={15} />
        </button>

        <div className="replay-speed" aria-label="Replay speed">
          {REPLAY_SPEEDS.map((candidate) => (
            <button
              key={candidate}
              type="button"
              aria-label={`${candidate} times speed`}
              aria-pressed={speed === candidate}
              className={speed === candidate ? "is-active" : ""}
              onClick={() => onSpeedChange(candidate)}
            >
              {candidate}x
            </button>
          ))}
        </div>

        {!liveAdvanced && !atLive && (
          <button
            type="button"
            className="replay-return-live"
            onClick={onReturnToLive}
          >
            <RotateCcw size={14} />
            Return to Live
          </button>
        )}
      </div>
    </section>
  );
}
