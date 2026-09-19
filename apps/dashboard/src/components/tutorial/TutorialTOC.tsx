import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useExploredSections } from '@/stores/tutorial-progress';
import { TUTORIAL_SECTIONS, type TutorialSection } from './sections';

interface TutorialTOCProps {
  /** Section IDs to render. Empty array means "render every section." */
  visibleIds: ReadonlySet<string>;
}

/**
 * Sticky right-rail Table of Contents.
 *
 * The active section is detected with IntersectionObserver against
 * `[data-tutorial-section]` anchors, biased toward the top of the
 * viewport so users see the section title above the fold get
 * highlighted, not whatever section happens to span the middle.
 *
 * We could have used `scroll` listeners + `getBoundingClientRect`, but
 * IntersectionObserver lets the browser batch the work and avoids
 * thrashing layout on long pages.
 */
export function TutorialTOC({ visibleIds }: TutorialTOCProps) {
  const { t } = useTranslation();
  const [activeId, setActiveId] = useState<string | null>(null);
  const explored = useExploredSections();

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const targets = TUTORIAL_SECTIONS.map((s) => document.getElementById(s.id)).filter((el): el is HTMLElement => !!el);
    if (targets.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        // Prefer the entry closest to the top of the viewport whose top has
        // crossed (or is about to cross) the rootMargin band. Falling back
        // to "any intersecting" gives us coverage when the page is shorter
        // than the visible area.
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (visible.length > 0) setActiveId(visible[0].target.id);
      },
      // Top band: 0px from top, then a generous bottom margin so a section
      // "stays active" until the next one reaches the band. Tuned so the
      // highlight feels predictable, not jumpy.
      { rootMargin: '0px 0px -75% 0px', threshold: [0, 0.1, 0.25] },
    );
    for (const el of targets) observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const handleClick = (id: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    const target = document.getElementById(id);
    if (!target) return;
    target.scrollIntoView({ behavior: 'smooth', block: 'start' });
    setActiveId(id);
    // Push the deep link into the URL so the user can copy/paste a link
    // straight to the section without the browser scrolling on reload.
    if (typeof window !== 'undefined' && window.history?.replaceState) {
      window.history.replaceState(null, '', `#${id}`);
    }
  };

  const sections = TUTORIAL_SECTIONS.filter((s) => visibleIds.size === 0 || visibleIds.has(s.id));
  if (sections.length === 0) return null;

  return (
    <nav aria-label={t('tutorial.toc.aria')} className="hidden lg:block lg:w-56 lg:flex-shrink-0">
      <div className="sticky top-4 space-y-1">
        <div className="text-xs uppercase tracking-wide text-muted-foreground mb-2 px-2">
          {t('tutorial.toc.heading')}
        </div>
        <ul className="space-y-0.5">
          {sections.map((s: TutorialSection) => {
            const isActive = activeId === s.id;
            const isExplored = explored.has(s.id);
            return (
              <li key={s.id}>
                <a
                  href={`#${s.id}`}
                  onClick={handleClick(s.id)}
                  className={`block px-2 py-1.5 text-xs rounded-md transition-colors ${
                    isActive
                      ? 'bg-primary/10 text-primary font-medium'
                      : 'text-muted-foreground hover:text-foreground hover:bg-accent'
                  }`}
                  aria-current={isActive ? 'true' : undefined}
                >
                  <span className="inline-flex items-center gap-1.5">
                    <span
                      aria-hidden="true"
                      className={`inline-block w-1.5 h-1.5 rounded-full flex-shrink-0 ${
                        isExplored ? 'bg-emerald-500' : 'bg-muted-foreground/30'
                      }`}
                    />
                    {t(s.labelKey)}
                  </span>
                </a>
              </li>
            );
          })}
        </ul>
      </div>
    </nav>
  );
}
