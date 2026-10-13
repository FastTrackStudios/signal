"""Fast checks for the benchmark's accounting; no display needed."""
import unittest
from run import distribution, summarize


class Reports(unittest.TestCase):
    def test_stalls_are_retained(self):
        stats = distribution([8.0] * 99 + [750.0])
        self.assertEqual(stats['worst'], 750.0)
        self.assertEqual(stats['count'], 100)
        self.assertGreater(stats['mean'], 8.33)

    def test_empty_sample_is_not_a_fast_frame(self):
        self.assertIsNone(distribution([]))

    def test_empty_run_fails(self):
        with self.assertRaises(ValueError):
            summarize({'samples': [], 'budget_ms': 1000 / 120})

    def test_one_stall_fails_the_budget(self):
        samples = [dict(phase='zoom', frame=i, event_ms=1, update_ms=1,
                        layout_ms=value - 2, total_ms=value)
                   for i, value in enumerate([3, 3, 750])]
        report = summarize({'samples': samples, 'budget_ms': 1000 / 120})
        self.assertFalse(report['dom_budget_pass'])
        self.assertEqual(report['phases']['zoom']['over_budget'], 1)
        self.assertEqual(report['phases']['zoom']['total_ms']['worst'], 750)


if __name__ == '__main__':
    unittest.main()
