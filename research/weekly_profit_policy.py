"""Share the weekly rotation rule and cost assumptions across price feeds."""
from weekly_rotation_forward import Account, DAY
from weekly_rotation_review import BASE_COST, STRESS_COST


WEEK = 7*DAY
MONDAY_OFFSET = 4*DAY
COSTS = {'base':BASE_COST, 'stress':STRESS_COST}


class WeeklySettlement(Account):
    keep_early_lock = True

    def rotation_signal(self, second, mark, weekly, target):
        super().signal(second,mark,weekly,target)
        if not self.keep_early_lock and self.sell == (second+1,'net_profit_lock'):
            self.sell = None

    def settle_week(self, second, weekly, target):
        if self.symbol and not self.sell:
            self.sell = second+1,'weekly_expiry'
            if weekly and target == self.symbol:
                self.buy = second+1,target

    def signal(self, second, mark, weekly, target):
        self.rotation_signal(second,mark,weekly,target)
        if (second+1)%WEEK == MONDAY_OFFSET:
            self.settle_week(second,weekly,target)


class WeeklyTrend(WeeklySettlement):
    keep_early_lock = False
