import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { PipelineVisualizer } from '@/components/PipelineVisualizer';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { NativeSelect } from '@/components/ui/native-select';
import { formatDateTime, formatTime } from '@/lib/format';
import { useTrackPageView } from '@/stores/telemetry';
import { useWebSocket } from '@/stores/websocket';
import type { IdentifiedEvent, VestigeEventType } from '@/types';
import { EVENT_TYPE_COLORS } from '@/types';

/**
 * Cap on the rendered JSON-preview length per event.
 *
 * The previous implementation sliced to 200 chars. We keep the cap to
 * stop a single 10kB content_preview from breaking the row layout, but
 * raise it slightly so the timestamp & node_type are usually visible
 * even after the slice.
 */
const PREVIEW_MAX_CHARS = 220;

/**
 * Extract the wire timestamp from a discriminated `VestigeEvent`.
 *
 * Almost every variant carries `timestamp: DateTime<Utc>` in its
 * `data` payload (see `crates/vestige-mcp/src/dashboard/events.rs`).
 * The synthetic `Connected` event injected by the WS client and the
 * `Heartbeat` variant don't, so we return null and skip rendering the
 * timestamp slot rather than printing `Invalid Date`.
 */
function eventTimestamp(event: IdentifiedEvent): Date | null {
  const data = event.data as { timestamp?: string } | null | undefined;
  if (!data?.timestamp) return null;
  const d = new Date(data.timestamp);
  return Number.isNaN(d.getTime()) ? null : d;
}

export function FeedPage() {
  useTrackPageView('feed');
  const { t, i18n } = useTranslation();
  // FeedPage SHOULD re-render whenever `events` changes, but not for unrelated
  // store fields (memoryCount, avgRetention, connection state). Per-field
  // selectors give us identity-stable references that React can compare cheaply.
  const events = useWebSocket((s) => s.events);
  const clearEvents = useWebSocket((s) => s.clearEvents);

  // Pause holds the list still while the user inspects something. We
  // snapshot the visible events at the moment of pause and keep showing
  // them until the user resumes. The store keeps growing in the
  // background — that's intentional, so the badge counter still moves.
  const [paused, setPaused] = useState(false);
  const [frozen, setFrozen] = useState<IdentifiedEvent[] | null>(null);
  // Type filter: 'all' or one of the event-type discriminators. Stored
  // as a string so `<select>` can write to it directly.
  const [typeFilter, setTypeFilter] = useState<string>('all');

  useEffect(() => {
    if (!paused) setFrozen(null);
  }, [paused]);

  const visible = useMemo(() => {
    const base = paused ? (frozen ?? []) : events;
    return typeFilter === 'all' ? base : base.filter((e) => e.type === typeFilter);
  }, [paused, frozen, events, typeFilter]);

  // Type options on the dropdown reflect the discriminators actually
  // seen in the current store (and `all`). Showing the universe of all
  // 17 event types would be noise — most users only see ~3.
  const availableTypes = useMemo(() => {
    const set = new Set<VestigeEventType>();
    for (const e of events) set.add(e.type);
    return Array.from(set).sort();
  }, [events]);

  const togglePause = () => {
    setPaused((prev) => {
      if (prev) return false;
      // Capture current events snapshot at pause time.
      setFrozen(events);
      return true;
    });
  };

  return (
    <div className="flex h-full">
      <div className="flex-1 flex flex-col min-w-0 border-r border-border">
        <div className="p-4 flex items-center justify-between border-b border-border gap-3 flex-wrap">
          <h1 className="text-sm font-bold text-foreground">{t('feed.title')}</h1>
          <div className="flex items-center gap-2">
            <NativeSelect
              value={typeFilter}
              onChange={(e) => setTypeFilter(e.target.value)}
              className="text-xs"
              aria-label={t('feed.filterByType')}
            >
              <option value="all">{t('feed.allTypes')}</option>
              {availableTypes.map((type) => (
                <option key={type} value={type}>
                  {type}
                </option>
              ))}
            </NativeSelect>
            <Button variant={paused ? 'secondary' : 'ghost'} size="sm" onClick={togglePause} aria-pressed={paused}>
              {paused ? t('feed.resume') : t('feed.pause')}
            </Button>
            <Badge variant="secondary">{visible.length}</Badge>
            <Button variant="ghost" size="sm" onClick={clearEvents}>
              {t('feed.clearAll')}
            </Button>
          </div>
        </div>

        {visible.length === 0 ? (
          <div className="flex-1 overflow-y-auto p-2">
            <EmptyState icon="◊" title={t('feed.noEvents')} description={t('feed.noEventsHint')} />
          </div>
        ) : (
          <ul className="flex-1 overflow-y-auto p-2 space-y-1 list-none" aria-label={t('feed.title')}>
            {visible.map((event) => {
              const ts = eventTimestamp(event);
              const color = EVENT_TYPE_COLORS[event.type] || '#8B95A5';
              const jsonBody = event.data ? JSON.stringify(event.data, null, 2).slice(0, PREVIEW_MAX_CHARS) : null;
              return (
                <li
                  key={event._id}
                  data-testid="feed-event"
                  className="flex items-start gap-3 px-3 py-2 rounded-lg hover:bg-accent transition min-w-0"
                >
                  <span
                    className="w-2 h-2 rounded-full mt-1.5 flex-shrink-0"
                    style={{ backgroundColor: color }}
                    aria-hidden="true"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      {ts && (
                        <time
                          dateTime={ts.toISOString()}
                          title={formatDateTime(ts, i18n.language)}
                          className="text-[10px] tabular-nums text-muted-foreground shrink-0"
                        >
                          {formatTime(ts, i18n.language)}
                        </time>
                      )}
                      <span className="text-xs font-medium truncate" style={{ color }}>
                        {event.type}
                      </span>
                    </div>
                    {jsonBody && (
                      <pre className="text-xs text-muted-foreground mt-1 overflow-x-auto max-w-full">{jsonBody}</pre>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="hidden lg:flex flex-col w-72 p-4">
        <h3 className="text-xs font-bold text-foreground mb-3">{t('settings.pipeline')}</h3>
        <PipelineVisualizer />
      </div>
    </div>
  );
}
