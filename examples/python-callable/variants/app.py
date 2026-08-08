def baseline(case: dict) -> dict:
    text = case["input"]["text"]
    label = "rejected" if "negative" in text else "accepted"
    return {"label": label, "reason": "Deterministic baseline."}


def candidate(case: dict) -> dict:
    return {"label": "accepted", "reason": "Deterministic candidate."}
