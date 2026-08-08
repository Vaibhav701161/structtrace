import argparse
import json
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--variant", choices=("baseline", "candidate"), required=True)
args = parser.parse_args()

for line in sys.stdin:
    request = json.loads(line)
    text = request["input"]["text"]
    label = "rejected" if args.variant == "baseline" and "negative" in text else "accepted"
    response = {
        "protocol": "structtrace.variant",
        "protocol_version": 1,
        "case_id": request["case_id"],
        "status": "ok",
        "output": {"label": label, "reason": f"{args.variant} deterministic result."},
    }
    print(json.dumps(response), flush=True)
