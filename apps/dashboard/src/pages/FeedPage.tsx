import { useTranslation } from 'react-i18next';
import { PipelineVisualizer } from '@/components/PipelineVisualizer';
import { EmptyState } from '@/components/ui/empty-state';
import { Badge } from '@/components/ui/badge';
import { useWebSocket } from '@/stores/websocket';
import { EVENT_TYPE_COLORS } from '@/types';

export function FeedPage() {
  const { t } = useTranslation();
  const { events, clearEvents } = useWebSocket();

  return (
    <div className="flex h-full">
      <div className="flex-1 flex flex-col min-w-0 border-r border-border">
        <div className="p-4 flex items-center justify-between border-b border-border">
          <h2 className="text-sm font-bold text-foreground">{t('feed.title')}</h2>
          <div className="flex items-center gap-3">
            <Badge variant="secondary">{events.length}</Badge>
            <button type="button" onClick={clearEvents} className="text-xs text-muted-foreground hover:text-foreground transition">
              {t('feed.clearAll')}
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1">
          {events.length === 0 && (
            <EmptyState
              icon="◊"
              title={t('feed.noEvents')}
              description={t('feed.noEventsHint')}
            />
          )}
          {events.map((event) => (
            <div key={event._id} className="flex items-start gap-3 px-3 py-2 rounded-lg hover:bg-accent transition min-w-0">
              <span
                className="w-2 h-2 rounded-full mt-1.5 flex-shrink-0"
                style={{ backgroundColor: EVENT_TYPE_COLORS[event.type] || '#8B95A5' }}
                aria-hidden="true"
              />
              <div className="min-w-0 flex-1">
                <span className="text-xs font-medium" style={{ color: EVENT_TYPE_COLORS[event.type] || '#8B95A5' }}>
                  {event.type}
                </span>
                {event.data && Object.keys(event.data).length > 0 && (
                  <pre className="text-xs text-muted-foreground mt-1 overflow-x-auto max-w-full">
                    {JSON.stringify(event.data, null, 2).slice(0, 200)}
                  </pre>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="hidden lg:flex flex-col w-72 p-4">
        <h3 className="text-xs font-bold text-foreground mb-3">{t('settings.pipeline')}</h3>
        <PipelineVisualizer />
      </div>
    </div>
  );
}
