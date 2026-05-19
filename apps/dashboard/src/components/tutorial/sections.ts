/**
 * Canonical list of tutorial sections.
 *
 * One place defines:
 *   - `id`        — DOM anchor (the section's `id="…"`) and TOC key
 *   - `labelKey`  — i18n key for the TOC entry / search hit / progress label
 *   - `keywords`  — extra search hints not present in the visible label,
 *                   so "how does the engine forget?" matches "decay" etc.
 *   - `quick`     — true if the section is part of the Quick (3 min) mode.
 *                   When the user picks Quick we hide everything else.
 *
 * If you add a tutorial section, register it here. Forgetting will make the
 * TOC and progress counter silently drop the section — a guarded design
 * choice: better than rendering "0 / undefined".
 */

export interface TutorialSection {
  id: string;
  labelKey: string;
  keywords: string[];
  quick: boolean;
}

export const TUTORIAL_SECTIONS: readonly TutorialSection[] = [
  {
    id: 'what-is',
    labelKey: 'tutorial.toc.whatIs',
    keywords: ['intro', 'what', 'memory', 'second brain', 'czym', 'mózg'],
    quick: true,
  },
  {
    id: 'analogies',
    labelKey: 'tutorial.toc.analogies',
    keywords: ['library', 'brain', 'web', 'biblioteka', 'sieć', 'metafora'],
    quick: true,
  },
  {
    id: 'how-memory-works',
    labelKey: 'tutorial.toc.howMemoryWorks',
    keywords: ['lifecycle', 'birth', 'connection', 'decay', 'cykl', 'życie'],
    quick: true,
  },
  {
    id: 'concepts',
    labelKey: 'tutorial.toc.concepts',
    keywords: ['retention', 'storage', 'dream', 'embedding', 'pojęcia'],
    quick: false,
  },
  {
    id: 'retention-curve',
    labelKey: 'tutorial.toc.retentionCurve',
    keywords: ['curve', 'fsrs', 'forecast', 'days', 'krzywa', 'wykres', 'symulator'],
    quick: false,
  },
  {
    id: 'science',
    labelKey: 'tutorial.toc.science',
    keywords: ['research', 'bjork', 'fsrs', 'ebbinghaus', 'nauka', 'badania'],
    quick: false,
  },
  {
    id: 'pages',
    labelKey: 'tutorial.toc.pages',
    keywords: ['guide', 'pages', 'navigation', 'strony', 'przewodnik'],
    quick: true,
  },
  {
    id: 'live-example',
    labelKey: 'tutorial.toc.liveExample',
    keywords: ['live', 'real', 'memory', 'forecast', 'przykład', 'twoje', 'na żywo'],
    quick: false,
  },
  {
    id: 'how-to',
    labelKey: 'tutorial.toc.howTo',
    keywords: ['use', 'routine', 'workflow', 'jak używać', 'rutyna'],
    quick: true,
  },
  {
    id: 'faq',
    labelKey: 'tutorial.toc.faq',
    keywords: ['questions', 'faq', 'help', 'pytania', 'pomoc'],
    quick: false,
  },
  {
    id: 'glossary',
    labelKey: 'tutorial.toc.glossary',
    keywords: ['terms', 'definitions', 'słowniczek', 'definicje', 'pojęcia'],
    quick: false,
  },
  {
    id: 'quiz',
    labelKey: 'tutorial.toc.quiz',
    keywords: ['test', 'check', 'verify', 'sprawdź', 'quiz'],
    quick: false,
  },
];

export function getSectionById(id: string): TutorialSection | undefined {
  return TUTORIAL_SECTIONS.find((s) => s.id === id);
}
