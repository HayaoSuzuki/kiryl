# Copyright (c) 2026 kiryl contributors
"""Pure integer application branches; no deliberately planted defect."""
def classify(value):
    if value < 0:
        if value % 2:
            path = "negative-odd"
        elif value < -65536:
            path = "negative-even-large"
        else:
            path = "negative-even-small"
    elif value == 0:
        path = "zero"
    elif value % 7 == 0:
        if value > 65536:
            path = "positive-seven-large"
        else:
            path = "positive-seven-small"
    elif value % 2:
        path = "positive-odd"
    else:
        path = "positive-even"
    return path
