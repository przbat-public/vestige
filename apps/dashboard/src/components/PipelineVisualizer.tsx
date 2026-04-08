import { useTranslation } from 'react-i18next';

interface StageInfo {
  nameKey: string;
  descKey: string;
  color: string;
}

const STAGES: StageInfo[] = [
  { nameKey: 'pipeline.overfetch', descKey: 'pipeline.overfetchDesc', color: '#00A8FF' },
  { nameKey: 'pipeline.rerank', descKey: 'pipeline.rerankDesc', color: '#818CF8' },
  { nameKey: 'pipeline.temporal', descKey: 'pipeline.temporalDesc', color: '#FFB800' },
  { nameKey: 'pipeline.accessibility', descKey: 'pipeline.accessibilityDesc', color: '#10B981' },
  { nameKey: 'pipeline.context', descKey: 'pipeline.contextDesc', color: '#A855F7' },
  { nameKey: 'pipeline.competition', descKey: 'pipeline.competitionDesc', color: '#FF4757' },
  { nameKey: 'pipeline.rrf', descKey: 'pipeline.rrfDesc', color: '#00FFD1' },
  { nameKey: 'pipeline.activation', descKey: 'pipeline.activationDesc', color: '#14E8C6' },
];

interface Props {
  activeStage?: number;
  className?: string;
}

export function PipelineVisualizer({ activeStage, className = '' }: Props) {
  const { t } = useTranslation();
  const highlightUpTo = activeStage ?? STAGES.length - 1;

  return (
    <ul className={`space-y-1.5 list-none p-0 m-0 ${className}`} aria-label={t('pipeline.ariaLabel')}>
      {STAGES.map((stage, i) => {
        const isActive = i <= highlightUpTo;
        return (
          <li
            key={stage.nameKey}
            className={`flex items-center gap-3 px-3 py-2 rounded-lg transition-all ${
              isActive ? 'glass border-l-2' : 'opacity-40'
            }`}
            style={isActive ? { borderLeftColor: stage.color } : undefined}
          >
            <div
              className={`w-2 h-2 rounded-full flex-shrink-0 ${isActive ? 'animate-pulse-glow' : ''}`}
              style={{ backgroundColor: isActive ? stage.color : '#4a4a7a' }}
            />
            <div className="min-w-0">
              <div className="text-xs font-medium text-foreground truncate">{t(stage.nameKey)}</div>
              <div className="text-xs text-muted-foreground truncate">{t(stage.descKey)}</div>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
