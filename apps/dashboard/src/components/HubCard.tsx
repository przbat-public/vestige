import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import type { Hub } from '@/types';

interface HubCardProps {
  hub: Hub;
}

export function HubCard({ hub }: HubCardProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const lastRegenerated = new Date(hub.lastRegeneratedAt);
  const [rangeStart, rangeEnd] = hub.dateRange;

  return (
    <Card className="space-y-3">
      <div className="space-y-1">
        <div className="flex items-start justify-between gap-3 flex-wrap">
          <p className="text-sm text-foreground break-words flex-1 min-w-0 whitespace-pre-wrap">{hub.content}</p>
          <div className="flex items-center gap-1.5 shrink-0">
            <Badge variant="secondary">
              {t('hubs.badge.children', {
                count: hub.childIds.length,
                defaultValue: '{{count}} memories',
              })}
            </Badge>
            {hub.regenerationCount > 0 && (
              <Badge variant="outline" className="text-muted-foreground">
                {t('hubs.badge.regenerated', {
                  count: hub.regenerationCount,
                  defaultValue: 'regenerated {{count}}×',
                })}
              </Badge>
            )}
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {t('hubs.subtitle', {
            last: lastRegenerated.toLocaleDateString(locale),
            method: hub.generationMethod,
            defaultValue: 'last refreshed {{last}} via {{method}}',
          })}
          {rangeStart && rangeEnd && (
            <>
              {' · '}
              {t('hubs.range', {
                from: new Date(rangeStart).toLocaleDateString(locale),
                to: new Date(rangeEnd).toLocaleDateString(locale),
                defaultValue: 'spans {{from}} → {{to}}',
              })}
            </>
          )}
        </p>
      </div>

      {hub.dominantTags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {hub.dominantTags.slice(0, 8).map((tag) => (
            <Badge key={tag} variant="outline" className="text-[10px] font-mono">
              {tag}
            </Badge>
          ))}
        </div>
      )}

      {hub.childIds.length > 0 && (
        <div className="text-[11px] text-muted-foreground flex flex-wrap gap-1">
          {hub.childIds.slice(0, 8).map((id) => (
            <a key={id} href={`/memories/${id}`} className="hover:underline font-mono" title={id}>
              {id.slice(0, 8)}
            </a>
          ))}
          {hub.childIds.length > 8 && <span>+{hub.childIds.length - 8}</span>}
        </div>
      )}
    </Card>
  );
}
