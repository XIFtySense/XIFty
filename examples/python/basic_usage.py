from pathlib import Path

import xifty


fixture = Path(__file__).resolve().parents[2] / "fixtures/minimal/happy.jpg"

print(f"XIFty version: {xifty.version()}")
print(xifty.extract(fixture, view="normalized"))
