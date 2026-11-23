"""Common inputs and observations for the parallel DOGE screens."""
import math
from dataclasses import dataclass, field
from datetime import date


DAY = 86400
HOLD = 120
LAST_SIGNAL = DAY - HOLD - 62


@dataclass
class Signal:
    eligible: int = 0
    observations: list = field(default_factory=list)

    def observe(self, sec, condition, a=0., b=0., c=0.):
        self.eligible += 1
        if condition:
            self.observations.append((sec, HOLD, a, b, c))


def start_microseconds(day):
    return (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY * 1_000_000


def bps(new, old):
    return 10000 * math.log(new / old)


def mid(quote):
    return (quote[1] + quote[2]) / 2


def spread(quote):
    return bps(quote[2], quote[1])


def bids_value(quote):
    return quote[1] * quote[3]
