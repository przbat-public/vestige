import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/stores/api';
import { queryKeys } from '@/stores/query';
import type { Memory } from '@/types';

/**
 * Picks one memory out of the user's database and visualises its current
 * retention.
 *
 * Why this exists: abstract concepts ("memories decay", "stability matters")
 * land harder when the user can see them play out on one of *their* memories.
 * Hands-down the most concrete moment in the tutorial.
 *
 * Selection rule: pick the highest-retention memory we can find — it
 * usually has a recent timestamp and a clear content snippet, which makes
 * a friendlier example than a half-faded fragment.
 *
 * Empty-state fallback: a "no memories yet" panel with a CTA to the
 * Briefing page, because telling the user about the system before they
 * have data is the typical first-visit experience.
 */
export function LiveMemoryExample() {
  const { t } = useTranslation();
  const query = useQuery({
    queryKey: queryKeys.memories({ limit: '20' }),
    queryFn: () => api.memories.list({ limit: '20' }),
  });

  if (query.isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t('tutorial.live.title')}</CardTitle>
          <CardDescription>{t('tutorial.live.subtitle')}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="text-xs text-muted-foreground">{t('common.loading')}</div>
        </CardContent>
      </Card>
    );
  }

  const memories: Memory[] = query.data?.memories ?? [];
  const pick = pickBestExample(memories);

  if (!pick) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t('tutorial.live.title')}</CardTitle>
          <CardDescription>{t('tutorial.live.empty.subtitle')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-muted-foreground leading-relaxed">{t('tutorial.live.empty.body')}</p>
          <Link
            to="/briefing"
            className="inline-flex items-center gap-1 text-xs font-medium text-primary hover:underline"
          >
            {t('tutorial.live.empty.cta')}
            <span aria-hidden="true">→</span>
          </Link>
        </CardContent>
      </Card>
    );
  }

  const retentionPct = (pick.retentionStrength * 100).toFixed(1);
  const ageDays = computeAgeDays(pick.createdAt);
  const preview = pick.content.length > 140 ? `${pick.content.slice(0, 140)}…` : pick.content;

  return (
    <Card className="border-primary/20 bg-primary/5">
      <CardHeader>
        <CardTitle>{t('tutorial.live.title')}</CardTitle>
        <CardDescription>{t('tutorial.live.subtitle')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <blockquote className="border-l-2 border-primary/40 pl-3 text-sm text-foreground italic leading-relaxed">
          {preview}
        </blockquote>
        <dl className="grid grid-cols-3 gap-3 text-xs">
          <div>
            <dt className="text-muted-foreground">{t('tutorial.live.retentionLabel')}</dt>
            <dd className="font-mono text-primary text-base">{retentionPct}%</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">{t('tutorial.live.ageLabel')}</dt>
            <dd className="font-mono text-foreground text-base">{t('tutorial.live.ageValue', { days: ageDays })}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">{t('tutorial.live.typeLabel')}</dt>
            <dd className="font-mono text-foreground text-base">{pick.nodeType}</dd>
          </div>
        </dl>
        <p className="text-xs text-muted-foreground leading-relaxed">
          {t('tutorial.live.explainer', { retention: retentionPct, days: ageDays })}
        </p>
        <Link
          to={`/memories?id=${encodeURIComponent(pick.id)}`}
          className="inline-flex items-center gap-1 text-xs font-medium text-primary hover:underline"
        >
          {t('tutorial.live.openCta')}
          <span aria-hidden="true">→</span>
        </Link>
      </CardContent>
    </Card>
  );
}

function pickBestExample(memories: Memory[]): Memory | null {
  if (memories.length === 0) return null;
  // Sort by retention descending; secondary sort by content length so we
  // bias toward memories that actually say something.
  const sorted = [...memories].sort((a, b) => {
    if (b.retentionStrength !== a.retentionStrength) return b.retentionStrength - a.retentionStrength;
    return b.content.length - a.content.length;
  });
  return sorted[0];
}

function computeAgeDays(iso: string): number {
  const created = Date.parse(iso);
  if (Number.isNaN(created)) return 0;
  const ms = Date.now() - created;
  return Math.max(0, Math.round(ms / (24 * 60 * 60 * 1000)));
}
