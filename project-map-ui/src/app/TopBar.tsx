import {
  ArrowLeft,
  ArrowRight,
  LocateFixed,
  RefreshCw,
  Search,
  Settings2,
} from "lucide-react";
import type {
  LifecycleStatus,
  ProjectMapNode,
  ProjectMapProjection,
} from "../data/types";
import type { ProjectMapView } from "../state/session";

interface TopBarProps {
  projection: ProjectMapProjection | null;
  activeView: ProjectMapView;
  breadcrumbBranchId: string;
  query: string;
  statusFilter: LifecycleStatus | "all";
  canGoBack: boolean;
  canGoForward: boolean;
  onViewChange: (view: ProjectMapView) => void;
  onGoBack: () => void;
  onGoForward: () => void;
  onQueryChange: (query: string) => void;
  onSearchCommit: () => void;
  onStatusFilterChange: (status: LifecycleStatus | "all") => void;
  onRefresh: () => void;
  onLocateCurrent: () => void;
  onOpenSettings: () => void;
}

function branchPath(
  projection: ProjectMapProjection | null,
  branchId: string,
): ProjectMapNode[] {
  if (!projection) {
    return [];
  }
  const byId = new Map(projection.nodes.map((node) => [node.id, node]));
  const path: ProjectMapNode[] = [];
  let cursor = byId.get(branchId);
  while (cursor && !path.some((node) => node.id === cursor!.id)) {
    path.unshift(cursor);
    cursor = cursor.parent ? byId.get(cursor.parent) : undefined;
  }
  return path;
}

export function TopBar({
  projection,
  activeView,
  breadcrumbBranchId,
  query,
  statusFilter,
  canGoBack,
  canGoForward,
  onViewChange,
  onGoBack,
  onGoForward,
  onQueryChange,
  onSearchCommit,
  onStatusFilterChange,
  onRefresh,
  onLocateCurrent,
  onOpenSettings,
}: TopBarProps) {
  const path = branchPath(projection, breadcrumbBranchId);
  return (
    <header className={`topbar ${activeView === "replay" ? "is-replay" : ""}`}>
      <div className="topbar-location">
        {activeView !== "replay" && (
          <div className="branch-history-controls" aria-label="Branch history">
            <button
              type="button"
              aria-label="Previous focused branch"
              title="Back to previous branch"
              disabled={!canGoBack}
              onClick={onGoBack}
            >
              <ArrowLeft size={15} />
            </button>
            <button
              type="button"
              aria-label="Next focused branch"
              title="Forward to next branch"
              disabled={!canGoForward}
              onClick={onGoForward}
            >
              <ArrowRight size={15} />
            </button>
          </div>
        )}
        <nav
          className="current-crumbs"
          aria-label={
            activeView === "replay"
              ? "Replay position"
              : activeView === "dependency"
              ? "Focused branch hierarchy path"
              : "Current branch path"
          }
        >
          {activeView === "replay" ? (
            <>
              <span>Project Map</span>
              <span>
                <span aria-hidden="true">›</span>
                <span>Accepted trajectory</span>
              </span>
            </>
          ) : path.length ? (
            path.map((node, index) => (
              <span key={node.id} className={node.is_current ? "is-current" : ""}>
                {index > 0 && <span aria-hidden="true">›</span>}
                <span>{node.title}</span>
              </span>
            ))
          ) : (
            <span>Project Map</span>
          )}
        </nav>
      </div>

      <div className="view-switcher" aria-label="Project Map view">
        <button
          type="button"
          className={activeView === "map" ? "is-active" : ""}
          aria-label="Map"
          aria-pressed={activeView === "map"}
          onClick={() => onViewChange("map")}
        >
          <span className="view-label-full">Map</span>
          <span className="view-label-short" aria-hidden="true">M</span>
        </button>
        <button
          type="button"
          className={activeView === "dependency" ? "is-active" : ""}
          aria-label="Dependency"
          aria-pressed={activeView === "dependency"}
          onClick={() => onViewChange("dependency")}
        >
          <span className="view-label-full">Dependency</span>
          <span className="view-label-short" aria-hidden="true">D</span>
        </button>
        <button
          type="button"
          className={activeView === "replay" ? "is-active" : ""}
          aria-label="Replay"
          aria-pressed={activeView === "replay"}
          onClick={() => onViewChange("replay")}
        >
          <span className="view-label-full">Replay</span>
          <span className="view-label-short" aria-hidden="true">R</span>
        </button>
      </div>

      <div className="topbar-actions">
        {activeView !== "replay" && (
          <>
            <label className="search-control">
              <Search size={15} aria-hidden="true" />
              <span className="sr-only">Search branches</span>
              <input
                id="branchSearch"
                type="search"
                aria-label="Search branches"
                value={query}
                placeholder="Search branches"
                autoComplete="off"
                onChange={(event) => onQueryChange(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    onSearchCommit();
                  }
                }}
              />
              <kbd>/</kbd>
            </label>

            <label className="status-filter-control">
              <span className="sr-only">Filter by lifecycle status</span>
              <select
                id="statusFilter"
                aria-label="Filter by lifecycle status"
                value={statusFilter}
                onChange={(event) =>
                  onStatusFilterChange(
                    event.currentTarget.value as LifecycleStatus | "all",
                  )
                }
              >
                <option value="all">All states</option>
                <option value="pending">Pending</option>
                <option value="in_progress">In progress</option>
                <option value="paused">Paused</option>
                <option value="complete">Complete</option>
                <option value="aborted">Aborted</option>
              </select>
            </label>

            <button
              type="button"
              className="icon-button desktop-action"
              aria-label="Locate current branch"
              title="Locate current branch (L)"
              onClick={onLocateCurrent}
            >
              <LocateFixed size={16} />
            </button>
            <button
              type="button"
              className="icon-button desktop-action"
              aria-label="Refresh accepted state"
              title="Refresh accepted state"
              onClick={onRefresh}
            >
              <RefreshCw size={16} />
            </button>
            <button
              id="openSettings"
              type="button"
              className="icon-button"
              aria-label="Open canvas settings"
              title="Canvas settings"
              onClick={onOpenSettings}
            >
              <Settings2 size={16} />
            </button>
          </>
        )}
        {activeView === "replay" && (
          <>
            <button
              type="button"
              className="icon-button"
              aria-label="Refresh Replay and accepted state"
              title="Refresh Replay and accepted state"
              onClick={onRefresh}
            >
              <RefreshCw size={16} />
            </button>
            <button
              id="openSettings"
              type="button"
              className="icon-button"
              aria-label="Open canvas settings"
              title="Canvas settings"
              onClick={onOpenSettings}
            >
              <Settings2 size={16} />
            </button>
          </>
        )}
      </div>
    </header>
  );
}
