import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EVENT, track } from '@/stores/telemetry';
import { saveQuizResult, useQuizResult } from '@/stores/tutorial-quiz';

/**
 * Four short multiple-choice questions that confirm the user picked up the
 * tutorial's load-bearing ideas. Not graded, not exported — purely a
 * "did this land?" check.
 *
 * Each question stores the *index* of the correct answer (zero-based).
 * Options live in i18n so EN and PL versions can pick their own
 * distractors that make sense in each language.
 *
 * Replay is supported. Resetting clears the saved result but keeps
 * the "you completed this once" memory off-screen — there's no
 * adversarial reason to penalise the user for double-checking.
 */

interface Question {
  /** i18n key for the question text. */
  promptKey: string;
  /** i18n keys for the four options. */
  optionKeys: [string, string, string, string];
  /** Index (0-3) of the correct option. */
  correctIndex: 0 | 1 | 2 | 3;
  /** i18n key for the "why this is correct" hint shown after answering. */
  hintKey: string;
}

const QUESTIONS: readonly Question[] = [
  {
    promptKey: 'tutorial.quiz.q1.prompt',
    optionKeys: ['tutorial.quiz.q1.opt0', 'tutorial.quiz.q1.opt1', 'tutorial.quiz.q1.opt2', 'tutorial.quiz.q1.opt3'],
    correctIndex: 2,
    hintKey: 'tutorial.quiz.q1.hint',
  },
  {
    promptKey: 'tutorial.quiz.q2.prompt',
    optionKeys: ['tutorial.quiz.q2.opt0', 'tutorial.quiz.q2.opt1', 'tutorial.quiz.q2.opt2', 'tutorial.quiz.q2.opt3'],
    correctIndex: 1,
    hintKey: 'tutorial.quiz.q2.hint',
  },
  {
    promptKey: 'tutorial.quiz.q3.prompt',
    optionKeys: ['tutorial.quiz.q3.opt0', 'tutorial.quiz.q3.opt1', 'tutorial.quiz.q3.opt2', 'tutorial.quiz.q3.opt3'],
    correctIndex: 0,
    hintKey: 'tutorial.quiz.q3.hint',
  },
  {
    promptKey: 'tutorial.quiz.q4.prompt',
    optionKeys: ['tutorial.quiz.q4.opt0', 'tutorial.quiz.q4.opt1', 'tutorial.quiz.q4.opt2', 'tutorial.quiz.q4.opt3'],
    correctIndex: 3,
    hintKey: 'tutorial.quiz.q4.hint',
  },
];

type Answer = number | null;

export function TutorialQuiz() {
  const { t } = useTranslation();
  const previous = useQuizResult();
  const [answers, setAnswers] = useState<Answer[]>(() => QUESTIONS.map(() => null));
  const [submitted, setSubmitted] = useState(false);

  const onSelect = (qIdx: number, optIdx: number) => {
    if (submitted) return;
    const next = [...answers];
    next[qIdx] = optIdx;
    setAnswers(next);
  };

  const allAnswered = answers.every((a) => a !== null);
  const score = answers.reduce<number>((acc, a, i) => acc + (a === QUESTIONS[i].correctIndex ? 1 : 0), 0);

  const onSubmit = () => {
    setSubmitted(true);
    saveQuizResult({ score, total: QUESTIONS.length, at: Date.now() });
    track(EVENT.tutorial_quiz_submit, { score, total: QUESTIONS.length });
  };

  const onReplay = () => {
    setAnswers(QUESTIONS.map(() => null));
    setSubmitted(false);
    track(EVENT.tutorial_quiz_replay);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('tutorial.quiz.title')}</CardTitle>
        <CardDescription>{t('tutorial.quiz.subtitle')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-5">
        {previous && !submitted && answers.every((a) => a === null) && (
          <div className="text-xs text-muted-foreground">
            {t('tutorial.quiz.previous', { score: previous.score, total: previous.total })}
          </div>
        )}

        {QUESTIONS.map((q, qIdx) => {
          const userAnswer = answers[qIdx];
          const correct = q.correctIndex;
          return (
            <fieldset key={q.promptKey} className="space-y-2 border-0 p-0 m-0">
              <legend className="text-sm font-medium text-foreground">
                {qIdx + 1}. {t(q.promptKey)}
              </legend>
              <div className="grid gap-1.5">
                {q.optionKeys.map((optKey, optIdx) => {
                  const isUser = userAnswer === optIdx;
                  const isCorrect = optIdx === correct;
                  let optClasses = 'border-border bg-background hover:bg-accent';
                  if (submitted && isCorrect) {
                    optClasses = 'border-emerald-500/50 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400';
                  } else if (submitted && isUser && !isCorrect) {
                    optClasses = 'border-rose-500/50 bg-rose-500/10 text-rose-700 dark:text-rose-400';
                  } else if (isUser) {
                    optClasses = 'border-primary/60 bg-primary/10 text-primary';
                  }
                  return (
                    <button
                      key={optKey}
                      type="button"
                      disabled={submitted}
                      onClick={() => onSelect(qIdx, optIdx)}
                      className={`text-left text-xs px-3 py-2 rounded-lg border transition ${optClasses} ${
                        submitted ? 'cursor-default' : 'cursor-pointer'
                      }`}
                      aria-pressed={isUser}
                    >
                      {t(optKey)}
                    </button>
                  );
                })}
              </div>
              {submitted && <p className="text-[11px] text-muted-foreground leading-relaxed pl-1">{t(q.hintKey)}</p>}
            </fieldset>
          );
        })}

        {!submitted ? (
          <Button onClick={onSubmit} disabled={!allAnswered} size="sm">
            {t('tutorial.quiz.submit')}
          </Button>
        ) : (
          <div className="flex items-center justify-between gap-3 pt-2 border-t border-border">
            <div className="text-sm font-medium text-foreground">
              {t('tutorial.quiz.scoreLine', { score, total: QUESTIONS.length })}
              <span className="ml-2 text-xs text-muted-foreground">
                {score === QUESTIONS.length
                  ? t('tutorial.quiz.scorePerfect')
                  : score >= QUESTIONS.length - 1
                    ? t('tutorial.quiz.scoreClose')
                    : t('tutorial.quiz.scoreReview')}
              </span>
            </div>
            <Button variant="ghost" size="sm" onClick={onReplay}>
              {t('tutorial.quiz.replay')}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
