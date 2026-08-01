import type {
  ProjectMapNode,
  ReplayResponse,
  ReplaySpeed,
  ReplayTransaction,
} from "./types";

export const REPLAY_SPEEDS: readonly ReplaySpeed[] = [0.5, 1, 2, 4];

export interface ReplayPosition {
  selectedSeq: number | null;
  branchFilter: string;
  followLive: boolean;
  speed: ReplaySpeed;
}

export interface ReplayBranchOption {
  id: string;
  title: string;
  label: string;
}

export function filterReplayTransactions(
  transactions: readonly ReplayTransaction[],
  branchFilter: string,
): ReplayTransaction[] {
  if (!branchFilter) {
    return [...transactions];
  }
  return transactions.filter(
    (transaction) =>
      transaction.subject === branchFilter ||
      transaction.affected_subjects.includes(branchFilter),
  );
}

export function sequenceForFilteredTimeline(
  transactions: readonly ReplayTransaction[],
  currentSeq: number | null,
): number | null {
  if (!transactions.length) {
    return null;
  }
  if (currentSeq !== null) {
    const exact = transactions.find(
      (transaction) => transaction.seq === currentSeq,
    );
    if (exact) {
      return exact.seq;
    }
    for (let index = transactions.length - 1; index >= 0; index -= 1) {
      if (transactions[index].seq <= currentSeq) {
        return transactions[index].seq;
      }
    }
  }
  return transactions[0].seq;
}

export function changeReplayBranchFilter(
  position: ReplayPosition,
  transactions: readonly ReplayTransaction[],
  branchFilter: string,
  liveEventSeq: number,
): ReplayPosition {
  const filtered = filterReplayTransactions(transactions, branchFilter);
  const selectedSeq = sequenceForFilteredTimeline(
    filtered.length || !branchFilter ? filtered : transactions,
    position.selectedSeq,
  );
  return {
    ...position,
    branchFilter,
    selectedSeq,
    followLive:
      !branchFilter &&
      selectedSeq !== null &&
      selectedSeq === liveEventSeq,
  };
}

export function reconcileReplayPosition(
  position: ReplayPosition,
  transactions: readonly ReplayTransaction[],
  liveEventSeq: number,
): ReplayPosition {
  if (!transactions.length) {
    return {
      ...position,
      selectedSeq: null,
      followLive: false,
    };
  }
  if (position.followLive && !position.branchFilter) {
    return {
      ...position,
      selectedSeq: transactions[transactions.length - 1].seq,
      followLive: true,
    };
  }
  const filtered = filterReplayTransactions(
    transactions,
    position.branchFilter,
  );
  return {
    ...position,
    selectedSeq: sequenceForFilteredTimeline(
      filtered.length || !position.branchFilter ? filtered : transactions,
      position.selectedSeq,
    ),
    followLive: false,
  };
}

export function selectReplaySequence(
  position: ReplayPosition,
  sequence: number,
  liveEventSeq: number,
): ReplayPosition {
  return {
    ...position,
    selectedSeq: sequence,
    followLive:
      !position.branchFilter && sequence === liveEventSeq,
  };
}

export function returnReplayToLive(
  position: ReplayPosition,
  transactions: readonly ReplayTransaction[],
  liveEventSeq: number,
): ReplayPosition {
  const latest =
    transactions.find((transaction) => transaction.seq === liveEventSeq) ??
    transactions[transactions.length - 1];
  return {
    ...position,
    branchFilter: "",
    selectedSeq: latest?.seq ?? null,
    followLive: Boolean(latest),
  };
}

export function replayBranchOptions(
  transactions: readonly ReplayTransaction[],
  liveNodes: readonly ProjectMapNode[],
  reconstructedNodes: readonly ProjectMapNode[],
): ReplayBranchOption[] {
  const ids = new Set<string>();
  liveNodes.forEach((node) => ids.add(node.id));
  reconstructedNodes.forEach((node) => ids.add(node.id));
  for (const transaction of transactions) {
    if (transaction.subject) {
      ids.add(transaction.subject);
    }
    transaction.affected_subjects.forEach((id) => {
      if (id) {
        ids.add(id);
      }
    });
  }
  const liveTitles = new Map(liveNodes.map((node) => [node.id, node.title]));
  const reconstructedTitles = new Map(
    reconstructedNodes.map((node) => [node.id, node.title]),
  );
  return [...ids]
    .sort((left, right) => left.localeCompare(right))
    .map((id) => {
      const title =
        reconstructedTitles.get(id) ?? liveTitles.get(id) ?? "";
      return {
        id,
        title,
        label: title ? `${title} · ${id}` : id,
      };
    });
}

export function shouldCacheReplaySnapshot(response: ReplayResponse): boolean {
  return response.reconstruction.status !== "unavailable";
}

export function shouldAnimateReplayTransition(
  previousSeq: number | null,
  nextSeq: number,
  reducedMotion: boolean,
): boolean {
  return (
    !reducedMotion &&
    previousSeq !== null &&
    Math.abs(previousSeq - nextSeq) === 1
  );
}
