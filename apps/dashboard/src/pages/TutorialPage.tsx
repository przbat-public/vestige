import { useTranslation } from 'react-i18next';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

interface StepProps {
  number: number;
  title: string;
  description: string;
  tip?: string;
}

function Step({ number, title, description, tip }: StepProps) {
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
      </CardContent>
    </Card>
  );
}

interface ConceptProps {
  emoji: string;
  title: string;
  explanation: string;
}

function Concept({ emoji, title, explanation }: ConceptProps) {
  return (
    <div className="flex gap-3 p-3 rounded-lg bg-card border border-border">
      <span className="text-2xl flex-shrink-0" role="img">
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
  answer: string;
}

function FaqItem({ question, answer }: FaqItemProps) {
  return (
    <div className="border-b border-border pb-3 last:border-0">
      <div className="text-sm font-medium text-foreground">{question}</div>
      <div className="text-xs text-muted-foreground leading-relaxed mt-1">{answer}</div>
    </div>
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
      <span className="text-3xl flex-shrink-0">{emoji}</span>
      <div>
        <div className="text-sm font-semibold text-foreground">{title}</div>
        <div className="text-sm text-muted-foreground leading-relaxed mt-1">{description}</div>
      </div>
    </div>
  );
}

export function TutorialPage() {
  const { t } = useTranslation();

  const steps = [
    {
      title: t('tutorial.steps.graph.title'),
      description: t('tutorial.steps.graph.desc'),
      tip: t('tutorial.steps.graph.tip'),
    },
    {
      title: t('tutorial.steps.memories.title'),
      description: t('tutorial.steps.memories.desc'),
      tip: t('tutorial.steps.memories.tip'),
    },
    { title: t('tutorial.steps.timeline.title'), description: t('tutorial.steps.timeline.desc') },
    { title: t('tutorial.steps.feed.title'), description: t('tutorial.steps.feed.desc') },
    {
      title: t('tutorial.steps.explore.title'),
      description: t('tutorial.steps.explore.desc'),
      tip: t('tutorial.steps.explore.tip'),
    },
    { title: t('tutorial.steps.intentions.title'), description: t('tutorial.steps.intentions.desc') },
    { title: t('tutorial.steps.stats.title'), description: t('tutorial.steps.stats.desc') },
    {
      title: t('tutorial.steps.settings.title'),
      description: t('tutorial.steps.settings.desc'),
      tip: t('tutorial.steps.settings.tip'),
    },
  ];

  const concepts = [
    { emoji: '🧠', title: t('tutorial.concepts.retention.title'), explanation: t('tutorial.concepts.retention.desc') },
    {
      emoji: '💪',
      title: t('tutorial.concepts.dualStrength.title'),
      explanation: t('tutorial.concepts.dualStrength.desc'),
    },
    { emoji: '😴', title: t('tutorial.concepts.dream.title'), explanation: t('tutorial.concepts.dream.desc') },
    {
      emoji: '🔗',
      title: t('tutorial.concepts.connections.title'),
      explanation: t('tutorial.concepts.connections.desc'),
    },
    { emoji: '📉', title: t('tutorial.concepts.decay.title'), explanation: t('tutorial.concepts.decay.desc') },
    { emoji: '🏷️', title: t('tutorial.concepts.types.title'), explanation: t('tutorial.concepts.types.desc') },
    {
      emoji: '⏰',
      title: t('tutorial.concepts.intentions.title'),
      explanation: t('tutorial.concepts.intentions.desc'),
    },
    { emoji: '🔍', title: t('tutorial.concepts.search.title'), explanation: t('tutorial.concepts.search.desc') },
    { emoji: '🪞', title: t('tutorial.concepts.reflect.title'), explanation: t('tutorial.concepts.reflect.desc') },
    {
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

  const faq = [
    { question: t('tutorial.faq.q1'), answer: t('tutorial.faq.a1') },
    { question: t('tutorial.faq.q2'), answer: t('tutorial.faq.a2') },
    { question: t('tutorial.faq.q3'), answer: t('tutorial.faq.a3') },
    { question: t('tutorial.faq.q4'), answer: t('tutorial.faq.a4') },
    { question: t('tutorial.faq.q5'), answer: t('tutorial.faq.a5') },
    { question: t('tutorial.faq.q6'), answer: t('tutorial.faq.a6') },
    { question: t('tutorial.faq.q7'), answer: t('tutorial.faq.a7') },
    { question: t('tutorial.faq.q8'), answer: t('tutorial.faq.a8') },
  ];

  return (
    <div className="p-4 space-y-8 overflow-y-auto h-full w-full">
      {/* Header */}
      <div>
        <h1 className="text-xl font-bold text-foreground">{t('tutorial.title')}</h1>
        <p className="text-sm text-muted-foreground mt-1 leading-relaxed max-w-2xl">{t('tutorial.intro')}</p>
      </div>

      {/* What is Vestige */}
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

      {/* Analogies */}
      <div>
        <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.analogiesTitle')}</h2>
        <div className="grid gap-3">
          {analogies.map((a) => (
            <Analogy key={a.title} emoji={a.emoji} title={a.title} description={a.description} />
          ))}
        </div>
      </div>

      {/* How it works step by step */}
      <Card>
        <CardHeader>
          <CardTitle>{t('tutorial.howMemoryWorks.title')}</CardTitle>
          <CardDescription>{t('tutorial.howMemoryWorks.subtitle')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-3">
            <div className="flex gap-3 items-start">
              <span className="w-7 h-7 rounded-full bg-emerald-500/15 text-emerald-500 text-xs font-bold flex items-center justify-center flex-shrink-0">
                1
              </span>
              <div>
                <div className="text-sm font-medium text-foreground">{t('tutorial.howMemoryWorks.s1.title')}</div>
                <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">
                  {t('tutorial.howMemoryWorks.s1.desc')}
                </div>
              </div>
            </div>
            <div className="flex gap-3 items-start">
              <span className="w-7 h-7 rounded-full bg-blue-500/15 text-blue-500 text-xs font-bold flex items-center justify-center flex-shrink-0">
                2
              </span>
              <div>
                <div className="text-sm font-medium text-foreground">{t('tutorial.howMemoryWorks.s2.title')}</div>
                <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">
                  {t('tutorial.howMemoryWorks.s2.desc')}
                </div>
              </div>
            </div>
            <div className="flex gap-3 items-start">
              <span className="w-7 h-7 rounded-full bg-violet-500/15 text-violet-500 text-xs font-bold flex items-center justify-center flex-shrink-0">
                3
              </span>
              <div>
                <div className="text-sm font-medium text-foreground">{t('tutorial.howMemoryWorks.s3.title')}</div>
                <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">
                  {t('tutorial.howMemoryWorks.s3.desc')}
                </div>
              </div>
            </div>
            <div className="flex gap-3 items-start">
              <span className="w-7 h-7 rounded-full bg-amber-500/15 text-amber-500 text-xs font-bold flex items-center justify-center flex-shrink-0">
                4
              </span>
              <div>
                <div className="text-sm font-medium text-foreground">{t('tutorial.howMemoryWorks.s4.title')}</div>
                <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">
                  {t('tutorial.howMemoryWorks.s4.desc')}
                </div>
              </div>
            </div>
            <div className="flex gap-3 items-start">
              <span className="w-7 h-7 rounded-full bg-rose-500/15 text-rose-500 text-xs font-bold flex items-center justify-center flex-shrink-0">
                5
              </span>
              <div>
                <div className="text-sm font-medium text-foreground">{t('tutorial.howMemoryWorks.s5.title')}</div>
                <div className="text-xs text-muted-foreground leading-relaxed mt-0.5">
                  {t('tutorial.howMemoryWorks.s5.desc')}
                </div>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Key Concepts */}
      <div>
        <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.conceptsTitle')}</h2>
        <div className="grid gap-3 sm:grid-cols-2">
          {concepts.map((c) => (
            <Concept key={c.title} emoji={c.emoji} title={c.title} explanation={c.explanation} />
          ))}
        </div>
      </div>

      {/* The Science Behind It */}
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

      {/* Pages Guide */}
      <div>
        <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.pagesTitle')}</h2>
        <div className="space-y-3">
          {steps.map((step, i) => (
            <Step key={step.title} number={i + 1} title={step.title} description={step.description} tip={step.tip} />
          ))}
        </div>
      </div>

      {/* How to use it */}
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

      {/* FAQ */}
      <div>
        <h2 className="text-lg font-semibold text-foreground mb-4">{t('tutorial.faqTitle')}</h2>
        <Card>
          <CardContent className="space-y-3 pt-4">
            {faq.map((item) => (
              <FaqItem key={item.question} question={item.question} answer={item.answer} />
            ))}
          </CardContent>
        </Card>
      </div>

      {/* Glossary */}
      <Card>
        <CardHeader>
          <CardTitle>{t('tutorial.glossary.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 text-xs">
          <div>
            <span className="font-medium text-foreground">Retention</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.retention')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Storage Strength</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.storage')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Retrieval Strength</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.retrieval')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">FSRS</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.fsrs')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Embedding</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.embedding')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Dream Cycle</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.dream')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Spreading Activation</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.activation')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Consolidation</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.consolidation')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">Node Type</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.nodeType')}</span>
          </div>
          <div>
            <span className="font-medium text-foreground">MCP</span> —{' '}
            <span className="text-muted-foreground">{t('tutorial.glossary.mcp')}</span>
          </div>
        </CardContent>
      </Card>

      {/* Footer */}
      <p className="text-xs text-muted-foreground text-center pb-4">{t('tutorial.footer')}</p>
    </div>
  );
}
