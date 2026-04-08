import { useTranslation } from 'react-i18next';
import type { DreamResult } from '@/types';

interface Props {
  result: DreamResult;
}

export function DreamResultPanel({ result }: Props) {
  const { t } = useTranslation();

  return (
    <div className="glass rounded-lg p-3 space-y-2">
      <div className="grid grid-cols-3 gap-2 text-xs">
        <div>
          {t('dream.replayed')}: <span className="text-foreground">{result.memoriesReplayed}</span>
        </div>
        <div>
          {t('dream.connections')}: <span className="text-foreground">{result.connectionsPersisted}</span>
        </div>
        <div>
          {t('dream.insights')}: <span className="text-foreground">{result.insights.length}</span>
        </div>
        <div>
          {t('dream.contradictions')}: <span className="text-danger">{result.contradictions.length}</span>
        </div>
        <div>
          {t('dream.demoted')}: <span className="text-danger">{result.memoriesDemoted.length}</span>
        </div>
        <div>
          {t('dream.duration')}: <span className="text-foreground">{result.stats.duration_ms}ms</span>
        </div>
      </div>
      {result.contradictions.length > 0 && (
        <div className="space-y-1 mt-2">
          <div className="text-xs text-danger font-medium">{t('dream.contradictions')}:</div>
          {result.contradictions.map((c) => (
            <div
              key={`${c.survivorId}-${c.demotedId}`}
              className="text-xs text-muted-foreground bg-danger/5 rounded p-1.5 overflow-wrap-anywhere"
            >
              {c.reason} ({t('dream.similarity', { value: c.similarity.toFixed(3) })})
            </div>
          ))}
        </div>
      )}
      {result.insights.length > 0 && (
        <div className="space-y-1 mt-1">
          <div className="text-xs text-primary font-medium">{t('dream.insights')}:</div>
          {result.insights.map((ins) => (
            <div key={`${ins.type}-${ins.insight}`} className="text-xs text-muted-foreground">
              {ins.insight}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
