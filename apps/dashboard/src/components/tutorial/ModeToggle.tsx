import { useTranslation } from 'react-i18next';
import { SegmentedControl } from '@/components/ui/segmented-control';
import { EVENT, track } from '@/stores/telemetry';

export type TutorialMode = 'quick' | 'full';

interface ModeToggleProps {
  value: TutorialMode;
  onChange: (mode: TutorialMode) => void;
}

/**
 * Quick (3 min) vs Full (15 min) mode picker.
 *
 * Quick hides the deep-science section, the glossary, and other reference
 * material. Full shows everything. The split is encoded on each section
 * (see `TutorialSection.quick`) so the toggle stays a one-line filter.
 *
 * We could have used an accordion ("all sections, collapsed by default")
 * but that hides the *existence* of full coverage from the user who picks
 * Quick. Mode toggle keeps both options visible at the top.
 */
export function ModeToggle({ value, onChange }: ModeToggleProps) {
  const { t } = useTranslation();
  const options = [
    { value: 'quick' as const, label: t('tutorial.mode.quick') },
    { value: 'full' as const, label: t('tutorial.mode.full') },
  ];
  return (
    <SegmentedControl
      value={value}
      onChange={(v) => {
        track(EVENT.tutorial_mode_change, { mode: v });
        onChange(v);
      }}
      options={options}
    />
  );
}
