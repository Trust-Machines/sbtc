(define-constant deployer tx-sender)

(define-read-only (invariant-signers-always-protocol-caller)
  (unwrap-panic (map-get? protocol-contracts .sbtc-bootstrap-signers)))

(define-read-only (invariant-deposit-always-protocol-caller)
  (unwrap-panic (map-get? protocol-contracts .sbtc-deposit)))

(define-read-only (invariant-withdrawal-always-protocol-caller)
  (unwrap-panic (map-get? protocol-contracts .sbtc-withdrawal)))

(define-read-only (invariant-protocol-caller-some-true (caller principal))
  (if (is-some (map-get? protocol-contracts caller))
    (unwrap-panic (map-get? protocol-contracts caller))
    true))

(define-read-only (invariant-withdraw-req-id-some (id uint))
  (if
    (and
      (<= id (var-get last-withdrawal-request-id))
      (> id u0))
    (is-some (map-get? withdrawal-requests id))
    true))

(define-read-only (invariant-withdraw-req-id-none (id uint))
  (if
    (or
      (> id (var-get last-withdrawal-request-id))
      (is-eq id u0))
    (is-none (map-get? withdrawal-requests id))
    true))

(define-read-only (invariant-last-withraw-req-id-eq-calls)
  (let (
      (num-calls-withdraw-req
        (default-to
          u0
          (get called (map-get? context "create-withdrawal-request"))))
    )
    (is-eq (var-get last-withdrawal-request-id) num-calls-withdraw-req)))

(define-read-only (invariant-withdrawal-status-none (req-id uint))
  (let (
      (num-calls-withdraw-accept
        (default-to
          u0
          (get called (map-get? context "complete-withdrawal-accept"))))
      (num-calls-withdraw-reject
        (default-to
          u0
          (get called (map-get? context "complete-withdrawal-reject"))))
    )
    (if
      (and
        (is-eq num-calls-withdraw-accept u0)
        (is-eq num-calls-withdraw-reject u0))
      (is-none (map-get? withdrawal-status req-id))
      true)))

(define-read-only (invariant-current-sig-threshold-unchanged)
  (let (
      (num-calls-rotate-keys
        (default-to u0 (get called (map-get? context "rotate-keys"))))
    )
    (if
      (is-eq num-calls-rotate-keys u0)
      (is-eq (var-get current-signature-threshold) u0)
      true)))

(define-read-only (invariant-current-sig-principal-unchanged)
  (let (
      (num-calls-rotate-keys
        (default-to u0 (get called (map-get? context "rotate-keys"))))
    )
    (if 
      (is-eq num-calls-rotate-keys u0)
      (is-eq (var-get current-signer-principal) deployer)
      true)))

(define-read-only (invariant-current-agg-pubkey-unchanged)
  (let (
      (num-calls-rotate-keys
        (default-to u0 (get called (map-get? context "rotate-keys"))))
    )
    (if 
      (is-eq num-calls-rotate-keys u0)
      (is-eq (var-get current-aggregate-pubkey) 0x00)
      true)))

(define-read-only (invariant-multi-sig-address-true)
  (let (
      (num-calls-rotate-keys (default-to u0 (get called (map-get? context "rotate-keys"))))
    )
    (if
      (> num-calls-rotate-keys u0)
      (unwrap-panic (map-get? multi-sig-address (var-get current-signer-principal)))
      true)))
