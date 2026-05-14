import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes } from 'react-router';
import { Layout } from '@/components/Layout';
import { LoadingSpinner } from '@/components/ui/loading-spinner';

const GraphPage = lazy(() => import('@/pages/GraphPage').then((m) => ({ default: m.GraphPage })));
const MemoriesPage = lazy(() => import('@/pages/MemoriesPage').then((m) => ({ default: m.MemoriesPage })));
const ReviewPage = lazy(() => import('@/pages/ReviewPage').then((m) => ({ default: m.ReviewPage })));
const BriefingPage = lazy(() => import('@/pages/BriefingPage').then((m) => ({ default: m.BriefingPage })));
const TimelinePage = lazy(() => import('@/pages/TimelinePage').then((m) => ({ default: m.TimelinePage })));
const FeedPage = lazy(() => import('@/pages/FeedPage').then((m) => ({ default: m.FeedPage })));
const ExplorePage = lazy(() => import('@/pages/ExplorePage').then((m) => ({ default: m.ExplorePage })));
const IntentionsPage = lazy(() => import('@/pages/IntentionsPage').then((m) => ({ default: m.IntentionsPage })));
const TemporalPage = lazy(() => import('@/pages/TemporalPage').then((m) => ({ default: m.TemporalPage })));
const StatsPage = lazy(() => import('@/pages/StatsPage').then((m) => ({ default: m.StatsPage })));
const SettingsPage = lazy(() => import('@/pages/SettingsPage').then((m) => ({ default: m.SettingsPage })));
const TutorialPage = lazy(() => import('@/pages/TutorialPage').then((m) => ({ default: m.TutorialPage })));
const NotFoundPage = lazy(() => import('@/pages/NotFoundPage').then((m) => ({ default: m.NotFoundPage })));

function PageFallback() {
  return (
    <div className="flex-1 flex items-center justify-center h-full">
      <LoadingSpinner />
    </div>
  );
}

export default function App() {
  return (
    <Suspense fallback={<PageFallback />}>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Navigate to="graph" replace />} />
          <Route path="graph" element={<GraphPage />} />
          <Route path="memories" element={<MemoriesPage />} />
          <Route path="review" element={<ReviewPage />} />
          <Route path="briefing" element={<BriefingPage />} />
          <Route path="timeline" element={<TimelinePage />} />
          <Route path="feed" element={<FeedPage />} />
          <Route path="explore" element={<ExplorePage />} />
          <Route path="intentions" element={<IntentionsPage />} />
          <Route path="temporal" element={<TemporalPage />} />
          <Route path="stats" element={<StatsPage />} />
          <Route path="settings" element={<SettingsPage />} />
          <Route path="tutorial" element={<TutorialPage />} />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
      </Routes>
    </Suspense>
  );
}
