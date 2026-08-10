// security/open-redirect on the span-boundary axis (see ./README.md).
//
// FP PROBE: `completePurchase` redirects to a constant internal path — the target is not derived from
// anything a caller sends. `quoteShipping` reads the request but only ever answers with JSON. The
// class contains no redirect whose target a caller can influence.

interface CheckoutRequest {
  query: Record<string, string>;
  params: Record<string, string>;
}

interface CheckoutResponse {
  redirect(target: string): void;
  json(body: unknown): void;
}

export class CheckoutFlowController {
  private readonly successPath = '/checkout/thanks';

  completePurchase = (_req: CheckoutRequest, res: CheckoutResponse) => {
    res.redirect(this.successPath);
  };

  quoteShipping = (req: CheckoutRequest, res: CheckoutResponse) => {
    res.json({ zone: req.query.zone, weight: req.params.weight });
  };
}

// TP CONTROL — one function, and the redirect target is whatever the caller put in `returnTo`.
export function redirectAfterLogin(req: CheckoutRequest, res: CheckoutResponse) {
  res.redirect(req.query.returnTo);
}
