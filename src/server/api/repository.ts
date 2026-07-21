import type {
  IdempotencyRecord,
  MailboxPolicy,
  Postage,
  PostageStatus,
  Receipt,
  SenderRule,
} from "./domain";

/**
 * Outcome of an atomic compare-and-swap postage state transition.
 *
 * - "not-found": no postage record exists for the given messageId.
 * - "conflict": the postage exists but its current status did not match the
 *   expected status, so no transition was applied. `postage` reflects the
 *   actual current record so callers can build a deterministic error.
 * - "applied": the transition was applied atomically. `postage` reflects the
 *   updated record.
 */
export type PostageTransitionResult =
  | { outcome: "not-found" }
  | { outcome: "conflict"; postage: Postage }
  | { outcome: "applied"; postage: Postage };

export type AcquireIdempotencyResult =
  | { status: "acquired" }
  | { status: "in_progress" }
  | { status: "completed"; record: IdempotencyRecord & { state: "completed" } };

export interface ApiRepository {
  getPolicy(owner: string): Promise<MailboxPolicy | null>;
  setPolicy(owner: string, policy: MailboxPolicy): Promise<MailboxPolicy>;
  getSenderRule(owner: string, sender: string): Promise<SenderRule>;
  setSenderRule(owner: string, sender: string, rule: SenderRule): Promise<SenderRule>;
  getPostage(messageId: string): Promise<Postage | null>;
  setPostage(postage: Postage): Promise<Postage>;
  /**
   * Atomically transitions a postage record from `expectedStatus` to
   * `nextStatus`. Implementations MUST guarantee that concurrent callers
   * racing on the same messageId observe a single winner: exactly one call
   * receives `{ outcome: "applied" }` and every other concurrent/subsequent
   * call receives `{ outcome: "conflict" }` reflecting the terminal state.
   * This must not be implemented as a plain get-then-set, since that is
   * vulnerable to double-settlement under concurrent requests.
   */
  transitionPostage(
    messageId: string,
    expectedStatus: PostageStatus,
    nextStatus: PostageStatus,
  ): Promise<PostageTransitionResult>;
  getReceipt(messageId: string): Promise<Receipt | null>;
  setReceipt(receipt: Receipt): Promise<Receipt>;
  acquireIdempotencyRecord(key: string, leaseMs: number): Promise<AcquireIdempotencyResult>;
  getIdempotencyRecord(key: string): Promise<IdempotencyRecord | null>;
  setIdempotencyRecord(key: string, record: IdempotencyRecord): Promise<void>;

  getRelayQueueDepth(relayId: string): Promise<number>;
  getRelayRetryCount(relayId: string): Promise<number>;
  getRelayLastSuccessfulDelivery(relayId: string): Promise<string | null>;
  getRelayLastFailedDelivery(relayId: string): Promise<string | null>;
  getRelayDeadLetterCount(relayId: string): Promise<number>;
  getCounter(key: string): Promise<number>;
  incrementCounter(key: string, windowSeconds: number, amount?: number): Promise<number>;
}

export const defaultMailboxPolicy: MailboxPolicy = {
  allowUnknown: false,
  minimumPostage: "0",
  requireVerified: true,
};
