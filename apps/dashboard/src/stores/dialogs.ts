import { create } from 'zustand';
import { EVENT, track } from '@/stores/telemetry';

/**
 * Global UI-orchestration store for cross-page dialog and selection
 * intents. Replaces the previous `window.dispatchEvent`/CustomEvent
 * bridges that piped intent between unrelated component trees — those
 * had no TypeScript safety, no devtools traceability, and were trivial
 * to break by typo.
 *
 * The store exposes two independent slots:
 *
 *  - `addMemoryOpen` — boolean shown/hidden by `Layout` regardless of
 *    which page is currently mounted. Page-level "Add memory" CTAs flip
 *    it via `openAddMemory(trigger)`; the FAB and ⌘N do the same.
 *
 *  - `pendingSelectMemoryId` — one-shot intent "open the memory drawer
 *    on the memories page with this id". Set by the command palette when
 *    a memory hit is selected. `MemoriesPage` reads and clears it on
 *    mount / on store change so a single click never opens twice.
 *
 * Both slots are deliberately denormalised (vs a list/queue) — the
 * dashboard only needs the latest intent. If we ever need queueing, we
 * can swap the field type without touching the producers.
 */
type AddMemoryTrigger = 'fab' | 'cmd_n' | 'event' | 'palette';

interface DialogStore {
  addMemoryOpen: boolean;
  pendingSelectMemoryId: string | null;
  openAddMemory: (trigger: AddMemoryTrigger) => void;
  closeAddMemory: () => void;
  requestSelectMemory: (id: string) => void;
  consumeSelectMemory: () => void;
}

export const useDialogStore = create<DialogStore>((set) => ({
  addMemoryOpen: false,
  pendingSelectMemoryId: null,
  openAddMemory: (trigger) => {
    // Track at the producer so every entry point lands the same event —
    // previously each call-site duplicated `track(EVENT.add_memory_open, …)`,
    // which silently drifted across triggers.
    track(EVENT.add_memory_open, { trigger });
    set({ addMemoryOpen: true });
  },
  closeAddMemory: () => set({ addMemoryOpen: false }),
  requestSelectMemory: (id) => set({ pendingSelectMemoryId: id }),
  consumeSelectMemory: () => set({ pendingSelectMemoryId: null }),
}));
