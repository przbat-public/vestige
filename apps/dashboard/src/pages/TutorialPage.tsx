import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { GlossaryTerm } from '@/components/tutorial/GlossaryTerm';
import { GLOSSARY_KEYS, termDefKey, termLabelKey } from '@/components/tutorial/glossary';
import { InteractiveTour } from '@/components/tutorial/InteractiveTour';
import { LiveMemoryExample } from '@/components/tutorial/LiveMemoryExample';
import { ModeToggle, type TutorialMode } from '@/components/tutorial/ModeToggle';
import { PageCTA } from '@/components/tutorial/PageCTA';
import { RetentionCurveWidget } from '@/components/tutorial/RetentionCurveWidget';
import { TUTORIAL_SECTIONS } from '@/components/tutorial/sections';
import { TutorialProgress } from '@/components/tutorial/TutorialProgress';
import { TutorialQuiz } from '@/components/tutorial/TutorialQuiz';
import { TutorialSearch } from '@/components/tutorial/TutorialSearch';
import { TutorialTOC } from '@/components/tutorial/TutorialTOC';
import { useSectionTracking } from '@/components/tutorial/useSectionTracking';
import { WhatsNewBanner } from '@/components/tutorial/WhatsNewBanner';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { useTrackPageView } from '@/stores/telemetry';
import { useTourTaken } from '@/stores/tutorial-tour';
import packageJson from '../../package.json';

/* ---------- small presentational helpers ----------------------------- */

interface StepProps {
  number: number;
  title: string;
  description: string;
  tip?: string;
  cta?: { to: string; pageKey: string };
}

function Step({ number, title, description, tip, cta }: StepProps) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-3">
          <span className="w-8 h-8 rounded-full bg-primary/15 text-primary text-sm font-bold flex items-center justify-center flex-shrink-0">
            {number}
          </span>
          <CardTitle className="text-base">{title}</CardTitle>
        </div>
      </CardHeader>
      <CardContent className="pl-14">
        <CardDescription className="text-sm leading-relaxed">{description}</CardDescription>
        {tip && (
          <div className="mt-3 px-3 py-2 rounded-lg bg-primary/5 border border-primary/10 text-xs text-primary">
            {tip}
          </div>
        )}
        {cta && <PageCTA to={cta.to} pageKey={cta.pageKey} />}
      </CardContent>
    </Card>
  );
}

interface ConceptProps {
  emoji: string;
  title: string;
  explanation: React.ReactNode;
}

function Concept({ emoji, title, explanation }: ConceptProps) {
  return (
    <div className="flex gap-3 p-3 rounded-lg bg-card border border-border">
      <span className="text-2xl flex-shrink-0" role="img" aria-hidden="true">
        {emoji}
      </span>
      <div>
        <div className="text-sm font-medium text-foreground">{title}</div>
        <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">{explanation}</div>
      </div>
    </div>
  );
}

interface FaqItemProps {
  question: string;
  answer: React.ReactNode;
}

function FaqItem({ question, answer }: FaqItemProps) {
  return (
    <details className="group border-b border-border pb-3 last:border-0">
      <summary className="text-sm font-medium text-foreground cursor-pointer list-none flex items-start justify-between gap-3">
        <span>{question}</span>
        <span
          aria-hidden="true"
          className="text-muted-foreground text-xs mt-0.5 transition-transform group-open:rotate-180"
        >
          ▾
        </span>
      </summary>
      <div className="text-xs text-muted-foreground leading-relaxed mt-2">{answer}</div>
    </details>
  );
}

interface AnalogyProps {
  emoji: string;
  title: string;
  description: string;
}

function Analogy({ emoji, title, description }: AnalogyProps) {
  return (
    <div className="flex gap-3 p-4 rounded-xl bg-card border border-border">
      <span className="text-3xl flex-shrink-0" aria-hidden="true">
        {emoji}
      </span>
      <div>
        <div className="text-sm font-semibold text-foreground">{title}</div>
        <div className="text-sm text-muted-foreground leading-relaxed mt-1">{description}</div>
      </div>
    </div>
  );
}

/* ---------- main page ------------------------------------------------- */

const PAGE_ROUTES: Record<string, string> = {
  briefing: 'briefing',
  graph: 'graph',
  memories: 'memories',
  review: 'review',
  timeline: 'timeline',
  feed: 'feed',
  explore: 'explore',
  hubs: 'hubs',
  insights: 'insights',
  decisions: 'decisions',
  temporal: 'temporal',
  intentions: 'intentions',
  stats: 'stats',
  settings: 'settings',
};

export function TutorialPage() {
  useTrackPageView('tutorial');
  const { t } = useTranslation();
  const [mode, setMode] = useState<TutorialMode>('full');
  const [tourOpen, setTourOpen] = useState(false);
  const tourTaken = useTourTaken();

  // Visible section IDs change with mode. We compute this once per render
  // and feed it to every progress / TOC consumer that cares.
  const visibleIds = useMemo(() => {
    const set = new Set<string>();
    for (const s of TUTORIAL_SECTIONS) {
      if (mode === 'full' || s.quick) set.add(s.id);
    }
    return set;
  }, [mode]);

  useSectionTracking();

  const steps: Array<{ key: string; cta?: { to: string; pageKey: string } }> = [
    { key: 'briefing', cta: { to: PAGE_ROUTES.briefing, pageKey: 'briefing' } },
    { key: 'review', cta: { to: PAGE_ROUTES.review, pageKey: 'review' } },
    { key: 'graph', cta: { to: PAGE_ROUTES.graph, pageKey: 'graph' } },
    { key: 'memories', cta: { to: PAGE_ROUTES.memories, pageKey: 'memories' } },
    { key: 'timeline', cta: { to: PAGE_ROUTES.timeline, pageKey: 'timeline' } },
    { key: 'feed', cta: { to: PAGE_ROUTES.feed, pageKey: 'feed' } },
    { key: 'explore', cta: { to: PAGE_ROUTES.explore, pageKey: 'explore' } },
    { key: 'hubs', cta: { to: PAGE_ROUTES.hubs, pageKey: 'hubs' } },
    { key: 'insights', cta: { to: PAGE_ROUTES.insights, pageKey: 'insights' } },
    { key: 'decisions', cta: { to: PAGE_ROUTES.decisions, pageKey: 'decisions' } },
    { key: 'temporal', cta: { to: PAGE_ROUTES.temporal, pageKey: 'temporal' } },
    { key: 'intentions', cta: { to: PAGE_ROUTES.intentions, pageKey: 'intentions' } },
    { key: 'stats', cta: { to: PAGE_ROUTES.stats, pageKey: 'stats' } },
    { key: 'settings', cta: { to: PAGE_ROUTES.settings, pageKey: 'settings' } },
  ];

  // Concepts get inline glossary tooltips for the term that names them.
  // The list mirrors the original tutorial; we just enrich the explanation
  // with `<GlossaryTerm>` where it lands naturally in body copy.
  const concepts: Array<{ emoji: string; title: string; explanation: React.ReactNode; key: string }> = [
    {
      key: 'retention',
      emoji: '🧠',
      title: t('tutorial.concepts.retention.title'),
      explanation: (
        <>
          {t('tutorial.concepts.retention.desc')} <GlossaryTerm termKey="retention" />
        </>
      ),
    },
    {
      key: 'dualStrength',
      emoji: '💪',
      title: t('tutorial.concepts.dualStrength.title'),
      explanation: (
        <>
          {t('tutorial.concepts.dualStrength.desc')} <GlossaryTerm termKey="storage" /> ·{' '}
          <GlossaryTerm termKey="retrieval" />
        </>
      ),
    },
    {
      key: 'dream',
      emoji: '😴',
      title: t('tutorial.concepts.dream.title'),
      explanation: (
        <>
          {t('tutorial.concepts.dream.desc')} <GlossaryTerm termKey="dream" />
        </>
      ),
    },
    {
      key: 'connections',
      emoji: '🔗',
      title: t('tutorial.concepts.connections.title'),
      explanation: (
        <>
          {t('tutorial.concepts.connections.desc')} <GlossaryTerm termKey="activation" />
        </>
      ),
    },
    {
      key: 'decay',
      emoji: '📉',
      title: t('tutorial.concepts.decay.title'),
      explanation: t('tutorial.concepts.decay.desc'),
    },
    {
      key: 'types',
      emoji: '🏷️',
      title: t('tutorial.concepts.types.title'),
      explanation: (
        <>
          {t('tutorial.concepts.types.desc')} <GlossaryTerm termKey="nodeType" />
        </>
      ),
    },
    {
      key: 'intentions',
      emoji: '⏰',
      title: t('tutorial.concepts.intentions.title'),
      explanation: t('tutorial.concepts.intentions.desc'),
    },
    {
      key: 'search',
      emoji: '🔍',
      title: t('tutorial.concepts.search.title'),
      explanation: (
        <>
          {t('tutorial.concepts.search.desc')} <GlossaryTerm termKey="embedding" />
        </>
      ),
    },
    {
      key: 'reflect',
      emoji: '🪞',
      title: t('tutorial.concepts.reflect.title'),
      explanation: t('tutorial.concepts.reflect.desc'),
    },
    {
      key: 'confidence',
      emoji: '📊',
      title: t('tutorial.concepts.confidence.title'),
      explanation: t('tutorial.concepts.confidence.desc'),
    },
  ];

  const analogies = [
    { emoji: '📚', title: t('tutorial.analogies.library.title'), description: t('tutorial.analogies.library.desc') },
    { emoji: '🧑‍🔬', title: t('tutorial.analogies.brain.title'), description: t('tutorial.analogies.brain.desc') },
    { emoji: '🌐', title: t('tutorial.analogies.web.title'), description: t('tutorial.analogies.web.desc') },
  ];

  // FAQ answers get inline glossary tooltips where the term appears in a
  // single answer (q6 is the embedding one, q4 covers dream/consolidation).
  const faq: Array<{ q: string; a: React.ReactNode }> = [
    { q: t('tutorial.faq.q1'), a: t('tutorial.faq.a1') },
    { q: t('tutorial.faq.q2'), a: t('tutorial.faq.a2') },
    { q: t('tutorial.faq.q3'), a: t('tutorial.faq.a3') },
    {
      q: t('tutorial.faq.q4'),
      a: (
        <>
          {t('tutorial.faq.a4')} <GlossaryTerm termKey="dream" /> · <GlossaryTerm termKey="consolidation" />
        </>
      ),
    },
    {
      q: t('tutorial.faq.q5'),
      a: (
        <>
          {t('tutorial.faq.a5')} <GlossaryTerm termKey="fsrs" />
        </>
      ),
    },
    {
      q: t('tutorial.faq.q6'),
      a: (
        <>
          {t('tutorial.faq.a6')} <GlossaryTerm termKey="embedding" />
        </>
      ),
    },
    { q: t('tutorial.faq.q7'), a: t('tutorial.faq.a7') },
    {
      q: t('tutorial.faq.q8'),
      a: (
        <>
          {t('tutorial.faq.a8')} <GlossaryTerm termKey="mcp" />
        </>
      ),
    },
  ];

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-6xl px-4 py-4 flex gap-8">
        {/* Main column ------------------------------------------------- */}
        <div className="flex-1 min-w-0 space-y-8">
          {/* Header */}
          <header className="space-y-4">
            <div>
              <h1 className="text-xl font-bold text-foreground">{t('tutorial.title')}</h1>
              <p className="text-sm text-muted-foreground mt-1 leading-relaxed max-w-2xl">{t('tutorial.intro')}</p>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <ModeToggle value={mode} onChange={setMode} />
              <Button size="sm" variant={tourTaken ? 'ghost' : 'default'} onClick={() => setTourOpen(true)}>
                {tourTaken ? t('tutorial.tour.replay') : t('tutorial.tour.startCta')}
              </Button>
              <TutorialProgress visibleIds={visibleIds} />
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <TutorialSearch />
            </div>
          </header>

          <WhatsNewBanner currentVersion={packageJson.version} />

          {/* Sections ---------------------------------------------------- */}
          <section id="what-is" data-tutorial-section="what-is">
            <Card className="border-primary/20 bg-primary/5">
              <CardHeader>
                <CardTitle>{t('tutorial.whatIs.title')}</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <p className="text-sm text-foreground leading-relaxed">{t('tutorial.whatIs.p1')}</p>
                <p className="text-sm text-muted-foreground leading-relaxed">{t('tutorial.whatIs.p2')}</p>
                <p className="text-sm text-muted-foreground leading-relaxed">{t('tutorial.whatIs.p3')}</p>
              </CardContent>
            </Card>
          </section>

          <section id="analogies" data-tutorial-section="analogies">
            <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.analogiesTitle')}</h2>
            <div className="grid gap-3">
              {analogies.map((a) => (
                <Analogy key={a.title} emoji={a.emoji} title={a.title} description={a.description} />
              ))}
            </div>
          </section>

          <section id="how-memory-works" data-tutorial-section="how-memory-works">
            <Card>
              <CardHeader>
                <CardTitle>{t('tutorial.howMemoryWorks.title')}</CardTitle>
                <CardDescription>{t('tutorial.howMemoryWorks.subtitle')}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-3">
                  {[1, 2, 3, 4, 5].map((i) => (
                    <div key={i} className="flex gap-3 items-start">
                      <span
                        className={`w-7 h-7 rounded-full text-xs font-bold flex items-center justify-center flex-shrink-0 ${
                          [
                            'bg-emerald-500/15 text-emerald-500',
                            'bg-blue-500/15 text-blue-500',
                            'bg-violet-500/15 text-violet-500',
                            'bg-amber-500/15 text-amber-500',
                            'bg-rose-500/15 text-rose-500',
                          ][i - 1]
                        }`}
                      >
                        {i}
                      </span>
                      <div>
                        <div className="text-sm font-medium text-foreground">
                          {t(`tutorial.howMemoryWorks.s${i}.title`)}
                        </div>
                        <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">
                          {t(`tutorial.howMemoryWorks.s${i}.desc`)}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          </section>

          {mode === 'full' && (
            <section id="concepts" data-tutorial-section="concepts">
              <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.conceptsTitle')}</h2>
              <div className="grid gap-3 sm:grid-cols-2">
                {concepts.map((c) => (
                  <Concept key={c.key} emoji={c.emoji} title={c.title} explanation={c.explanation} />
                ))}
              </div>
            </section>
          )}

          {mode === 'full' && (
            <section id="retention-curve" data-tutorial-section="retention-curve">
              <RetentionCurveWidget />
            </section>
          )}

          {mode === 'full' && (
            <section id="science" data-tutorial-section="science">
              <Card>
                <CardHeader>
                  <CardTitle>{t('tutorial.science.title')}</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  <p className="text-sm text-muted-foreground leading-relaxed">{t('tutorial.science.p1')}</p>
                  <p className="text-sm text-muted-foreground leading-relaxed">{t('tutorial.science.p2')}</p>
                  <p className="text-sm text-muted-foreground leading-relaxed">{t('tutorial.science.p3')}</p>
                </CardContent>
              </Card>
            </section>
          )}

          <section id="pages" data-tutorial-section="pages">
            <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.pagesTitle')}</h2>
            <div className="space-y-3">
              {steps.map((step, i) => (
                <Step
                  key={step.key}
                  number={i + 1}
                  title={t(`tutorial.steps.${step.key}.title`)}
                  description={t(`tutorial.steps.${step.key}.desc`)}
                  tip={(() => {
                    const tip = t(`tutorial.steps.${step.key}.tip`, { defaultValue: '' });
                    return tip || undefined;
                  })()}
                  cta={step.cta}
                />
              ))}
            </div>
          </section>

          {mode === 'full' && (
            <section id="live-example" data-tutorial-section="live-example">
              <LiveMemoryExample />
            </section>
          )}

          <section id="how-to" data-tutorial-section="how-to">
            <Card>
              <CardHeader>
                <CardTitle>{t('tutorial.howTo.title')}</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2">
                <p className="text-sm text-foreground leading-relaxed">{t('tutorial.howTo.p1')}</p>
                <ul className="text-sm text-muted-foreground leading-relaxed list-disc pl-5 space-y-1.5">
                  <li>{t('tutorial.howTo.li1')}</li>
                  <li>{t('tutorial.howTo.li2')}</li>
                  <li>{t('tutorial.howTo.li3')}</li>
                  <li>{t('tutorial.howTo.li4')}</li>
                </ul>
              </CardContent>
            </Card>
          </section>

          {mode === 'full' && (
            <section id="faq" data-tutorial-section="faq">
              <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.faqTitle')}</h2>
              <Card>
                <CardContent className="space-y-3 pt-4">
                  {faq.map((item) => (
                    <FaqItem key={item.q} question={item.q} answer={item.a} />
                  ))}
                </CardContent>
              </Card>
            </section>
          )}

          {mode === 'full' && (
            <section id="glossary" data-tutorial-section="glossary">
              <Card>
                <CardHeader>
                  <CardTitle>{t('tutorial.glossary.title')}</CardTitle>
                </CardHeader>
                <CardContent className="space-y-2 text-xs">
                  {GLOSSARY_KEYS.map((key) => (
                    <div key={key}>
                      <span className="font-medium text-foreground">{t(termLabelKey(key))}</span> —{' '}
                      <span className="text-muted-foreground">{t(termDefKey(key))}</span>
                    </div>
                  ))}
                </CardContent>
              </Card>
            </section>
          )}

          {mode === 'full' && (
            <section id="quiz" data-tutorial-section="quiz">
              <TutorialQuiz />
            </section>
          )}

          {/* Footer */}
          <p className="text-xs text-muted-foreground text-center pb-4">{t('tutorial.footer')}</p>
        </div>

        {/* TOC --------------------------------------------------------- */}
        <TutorialTOC visibleIds={visibleIds} />
      </div>

      <InteractiveTour open={tourOpen} onClose={() => setTourOpen(false)} />
    </div>
  );
}
