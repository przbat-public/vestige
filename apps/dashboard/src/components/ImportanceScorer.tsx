import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { api } from '@/stores/api';
import { toast } from '@/stores/toast';
import type { ImportanceScore } from '@/types';

export function ImportanceScorer() {
  const { t } = useTranslation();
  const [text, setText] = useState('');
  const [result, setResult] = useState<ImportanceScore | null>(null);
  const [loading, setLoading] = useState(false);

  const score = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!text.trim()) return;
    setLoading(true);
    try {
      const res = await api.importance(text);
      setResult(res);
    } catch {
      toast(t('common.error'), 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="pt-8 border-t border-border">
      <h2 className="text-lg text-foreground font-semibold mb-4">{t('explore.importanceScorer')}</h2>
      <form onSubmit={score}>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t('explore.importancePlaceholder')}
          className="w-full h-24 px-4 py-3 rounded-xl text-sm bg-background border border-border text-foreground placeholder:text-muted-foreground resize-none focus:outline-none focus:ring-2 focus:ring-ring"
        />
        <Button type="submit" variant="dream" className="mt-2" disabled={loading}>
          {loading ? t('common.loading') : t('common.score')}
        </Button>
      </form>

      {result && (
        <Card className="mt-4">
          <div className="flex items-center gap-3 mb-4">
            <span className="text-3xl text-foreground font-bold tabular-nums">{result.composite.toFixed(2)}</span>
            <Badge variant={result.recommendation === 'save' ? 'success' : 'secondary'}>
              {result.recommendation === 'save' ? t('explore.resultSave') : t('explore.resultSkip')}
            </Badge>
          </div>
          <div className="grid grid-cols-4 gap-3">
            {Object.entries(result.channels).map(([channel, score]) => (
              <div key={channel}>
                <div className="text-xs text-muted-foreground mb-1.5 capitalize">{channel}</div>
                <div className="h-2 bg-muted rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full bg-primary transition-all duration-500"
                    style={{ width: `${score * 100}%` }}
                  />
                </div>
                <div className="text-xs text-muted-foreground mt-1 tabular-nums">{score.toFixed(2)}</div>
              </div>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}
