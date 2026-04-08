import { useTranslation } from 'react-i18next';
import type { RetentionDistribution } from '@/types';
import { retentionColor } from '@/types';

interface Props {
  distribution: RetentionDistribution;
}

export function GovernancePanel({ distribution }: Props) {
  const { t } = useTranslation();
  const maxCount = Math.max(...distribution.distribution.map((b) => b.count), 1);

  return (
    <div className="glass rounded-xl p-4 space-y-3">
      <h3 className="text-xs font-bold text-foreground">{t('governance.title')}</h3>

      <div className="flex items-end gap-1 h-16">
        {distribution.distribution.map((bucket, i) => {
          const height = (bucket.count / maxCount) * 100;
          return (
            <div
              key={bucket.range}
              className="flex-1 flex flex-col items-center gap-0.5"
              title={`${bucket.range}: ${bucket.count}`}
            >
              <div
                className="w-full rounded-t transition-all"
                style={{
                  height: `${Math.max(height, 2)}%`,
                  backgroundColor: retentionColor(i / 10 + 0.05),
                  opacity: 0.7,
                }}
              />
              <span className="text-xs text-muted-foreground">{bucket.count}</span>
            </div>
          );
        })}
      </div>
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>0%</span>
        <span>{t('governance.retentionDistribution')}</span>
        <span>100%</span>
      </div>

      {distribution.endangered.length > 0 ? (
        <div className="space-y-2 mt-2">
          <div className="flex items-center gap-2">
            <span className="text-xs text-danger font-medium">
              {t('governance.endangered', { count: distribution.endangered.length })}
            </span>
            <span className="text-xs text-muted-foreground">{t('governance.endangeredHint')}</span>
          </div>
          <div className="max-h-32 overflow-y-auto space-y-1">
            {distribution.endangered.slice(0, 20).map((mem) => (
              <div key={mem.id} className="flex items-center gap-2 text-xs bg-danger/5 rounded px-2 py-1">
                <span
                  className="w-1.5 h-1.5 rounded-full flex-shrink-0"
                  style={{ backgroundColor: retentionColor(mem.retentionStrength) }}
                />
                <span className="text-muted-foreground truncate flex-1">{mem.content}</span>
                <span className="text-danger flex-shrink-0">{(mem.retentionStrength * 100).toFixed(0)}%</span>
              </div>
            ))}
            {distribution.endangered.length > 20 && (
              <div className="text-xs text-muted-foreground text-center">
                {t('common.more', { count: distribution.endangered.length - 20 })}
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="text-xs text-success">{t('governance.allHealthy')}</div>
      )}
    </div>
  );
}
