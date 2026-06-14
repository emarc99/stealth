const actorSecurity = [{ ActorHeader: [] }];

export const openApiDocument = {
  openapi: "3.1.0",
  info: {
    title: "Stealth Mail API",
    version: "1.0.0",
    description:
      "Development API for mailbox policy, Stellar postage proofs, and delivery receipts.",
  },
  servers: [{ url: "/api/v1" }],
  components: {
    securitySchemes: {
      ActorHeader: {
        type: "apiKey",
        in: "header",
        name: "x-stealth-address",
        description:
          "Development actor transport. Production must derive this identity from a verified signed session.",
      },
    },
    schemas: {
      StellarAddress: {
        type: "string",
        pattern: "^G[A-Z2-7]{55}$",
      },
      Hash32: {
        type: "string",
        pattern: "^[a-f0-9]{64}$",
      },
      StroopAmount: {
        type: "string",
        pattern: "^(0|[1-9][0-9]*)$",
      },
      MailboxPolicy: {
        type: "object",
        required: ["allowUnknown", "minimumPostage", "requireVerified"],
        properties: {
          allowUnknown: { type: "boolean" },
          minimumPostage: { $ref: "#/components/schemas/StroopAmount" },
          requireVerified: { type: "boolean" },
        },
      },
    },
  },
  paths: {
    "/health": { get: { summary: "Read service health" } },
    "/protocol": { get: { summary: "Discover protocol capabilities" } },
    "/openapi.json": { get: { summary: "Read this OpenAPI document" } },
    "/policies/{owner}": {
      get: { summary: "Read mailbox policy" },
      put: { summary: "Replace mailbox policy", security: actorSecurity },
    },
    "/policies/{owner}/senders/{sender}": {
      get: { summary: "Read a sender override" },
      put: { summary: "Set a sender override", security: actorSecurity },
      delete: { summary: "Reset a sender override", security: actorSecurity },
    },
    "/policies/evaluate": {
      post: { summary: "Evaluate whether a sender can mail a recipient" },
    },
    "/postage": {
      post: { summary: "Submit a postage proof", security: actorSecurity },
    },
    "/postage/quote": {
      post: { summary: "Quote recipient postage requirements" },
    },
    "/postage/{messageId}": {
      get: { summary: "Read participant postage state", security: actorSecurity },
    },
    "/postage/{messageId}/settle": {
      post: { summary: "Settle pending postage", security: actorSecurity },
    },
    "/postage/{messageId}/refund": {
      post: { summary: "Mark pending postage for refund", security: actorSecurity },
    },
    "/receipts": {
      post: { summary: "Record message delivery", security: actorSecurity },
    },
    "/receipts/{messageId}": {
      get: { summary: "Read participant receipt state", security: actorSecurity },
    },
    "/receipts/{messageId}/read": {
      post: { summary: "Record recipient read acknowledgment", security: actorSecurity },
    },
  },
} as const;
