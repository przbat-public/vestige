import { cn } from '@/lib/utils';

interface SkeletonProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Width via Tailwind class (default `w-full`). */
  w?: string;
  /** Height via Tailwind class (default `h-4`). */
  h?: string;
  /** Border-radius via Tailwind class (default `rounded`). */
  rounded?: string;
}

/**
 * Single shimmer block. Use directly for tiny shapes (avatar, badge) or
 * compose the higher-level helpers (`SkeletonText`, `SkeletonCard`,
 * `SkeletonList`) for full-page placeholders.
 *
 * Skeletons beat spinners for perceived performance: a shape that resembles
 * the final content tells the user "this view is loading what you expect"
 * instead of "loading… something". Honors `prefers-reduced-motion` by
 * disabling the shimmer animation.
 */
export function Skeleton({ w = 'w-full', h = 'h-4', rounded = 'rounded', className, ...rest }: SkeletonProps) {
  return (
    <div
      aria-hidden="true"
      className={cn('bg-muted/60 animate-pulse motion-reduce:animate-none', w, h, rounded, className)}
      {...rest}
    />
  );
}

interface SkeletonTextProps {
  /** Number of lines (default 3). */
  lines?: number;
  /** Optional className applied to the wrapping stack. */
  className?: string;
}

/**
 * Multi-line text shimmer. Last line is rendered slightly shorter so it
 * looks like an actual paragraph rather than a perfect rectangle.
 */
export function SkeletonText({ lines = 3, className }: SkeletonTextProps) {
  return (
    <div className={cn('space-y-2', className)}>
      {Array.from({ length: lines }).map((_, i) => (
        <Skeleton
          // Last line is 70% wide so the shape reads as prose, not a bar.
          // biome-ignore lint/suspicious/noArrayIndexKey: stable count, no reorder
          key={i}
          w={i === lines - 1 && lines > 1 ? 'w-3/4' : 'w-full'}
          h="h-3"
        />
      ))}
    </div>
  );
}

interface SkeletonCardProps {
  className?: string;
  /** Show a circular avatar skeleton in the top-left corner. */
  withAvatar?: boolean;
  /** Number of body text lines (default 2). */
  lines?: number;
  /** Show a footer row of metadata pills (default true). */
  withFooter?: boolean;
}

/**
 * Card-shaped placeholder mimicking the dashboard's standard
 * `Card` layout: title row, body text, optional footer chips.
 */
export function SkeletonCard({ className, withAvatar = false, lines = 2, withFooter = true }: SkeletonCardProps) {
  return (
    <div className={cn('rounded-xl border border-border bg-card p-4 space-y-3', className)}>
      <div className="flex items-center gap-3">
        {withAvatar && <Skeleton w="w-9" h="h-9" rounded="rounded-full" />}
        <Skeleton w="w-1/2" h="h-4" />
      </div>
      <SkeletonText lines={lines} />
      {withFooter && (
        <div className="flex gap-2 pt-1">
          <Skeleton w="w-14" h="h-5" rounded="rounded-full" />
          <Skeleton w="w-20" h="h-5" rounded="rounded-full" />
          <Skeleton w="w-12" h="h-5" rounded="rounded-full" />
        </div>
      )}
    </div>
  );
}

interface SkeletonListProps {
  /** Number of skeleton cards to render (default 4). */
  count?: number;
  className?: string;
  /** Forwarded to each SkeletonCard. */
  cardProps?: Omit<SkeletonCardProps, 'className'>;
}

/**
 * Vertical list of skeleton cards. Use as a placeholder for paginated lists
 * (memories, hubs, insights, decisions). The default of 4 cards covers most
 * viewports without inducing layout shift when real content lands.
 */
export function SkeletonList({ count = 4, className, cardProps }: SkeletonListProps) {
  return (
    <div className={cn('space-y-2', className)} role="status" aria-busy="true">
      {Array.from({ length: count }).map((_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: stable count, no reorder
        <SkeletonCard key={i} {...cardProps} />
      ))}
      <span className="sr-only">Loading</span>
    </div>
  );
}
