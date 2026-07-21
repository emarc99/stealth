import { createFileRoute } from "@tanstack/react-router";
import { apiSuccess } from "@/server/api/response";
import { createRouteHandler } from "@/server/api/handler";

export const Route = createFileRoute("/api/v1/health")({
  server: {
    handlers: {
      GET: ({ request }) =>
        createRouteHandler({
          handler: () =>
            apiSuccess(request, {
              environment: import.meta.env.MODE,
              service: "stealth-mail-api",
              status: "ok",
              version: "v1",
            }),
        })(request),
    },
  },
});
