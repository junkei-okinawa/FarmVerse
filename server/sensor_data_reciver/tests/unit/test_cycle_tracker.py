import os
import sys
import unittest

# テスト対象へのパスを通す
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", ".."))

from protocol.cycle_tracker import CycleTracker


class TestCycleTracker(unittest.TestCase):
    def test_cycle_transitions_and_completion(self):
        tracker = CycleTracker()
        sender_mac = "01:02:03:04:05:06"

        state = tracker.observe_data(sender_mac, 10, now=1.0)
        self.assertEqual(state.cycle_state, "ReceivingData")
        self.assertEqual(state.cycle_seq_num, 10)
        self.assertFalse(state.hash_received)

        state = tracker.observe_hash(sender_mac, 11, now=2.0)
        self.assertEqual(state.cycle_state, "HashReceived")
        self.assertEqual(state.cycle_seq_num, 11)
        self.assertTrue(state.hash_received)

        state = tracker.observe_eof(sender_mac, 12, now=3.0)
        self.assertEqual(state.cycle_state, "EofReceived")
        self.assertEqual(state.cycle_seq_num, 11)
        self.assertTrue(state.eof_received)
        self.assertFalse(state.warning_emitted)

        state = tracker.complete_cycle(sender_mac, now=4.0)
        self.assertIsNotNone(state)
        self.assertEqual(state.cycle_state, "Completed")

    def test_eof_without_data_emits_single_warning(self):
        tracker = CycleTracker()
        sender_mac = "aa:bb:cc:dd:ee:ff"

        with self.assertLogs("protocol.cycle_tracker", level="WARNING") as logs:
            state = tracker.observe_eof(sender_mac, 21, now=1.0)

        self.assertTrue(
            any("EOF received before DATA/HASH" in message for message in logs.output)
        )
        self.assertEqual(len(logs.output), 1)
        self.assertEqual(state.cycle_state, "EofReceived")
        self.assertEqual(state.cycle_seq_num, 21)
        self.assertTrue(state.warning_emitted)

    def test_eof_after_data_without_hash_emits_missing_hash_warning(self):
        tracker = CycleTracker()
        sender_mac = "aa:bb:cc:dd:ee:01"

        tracker.observe_data(sender_mac, 20, now=0.5)

        with self.assertLogs("protocol.cycle_tracker", level="WARNING") as logs:
            state = tracker.observe_eof(sender_mac, 21, now=1.0)

        self.assertTrue(
            any("EOF received but HASH was not received" in message for message in logs.output)
        )
        self.assertEqual(len(logs.output), 1)
        self.assertEqual(state.cycle_state, "EofReceived")
        self.assertTrue(state.warning_emitted)

    def test_new_cycle_starts_after_completion(self):
        tracker = CycleTracker()
        sender_mac = "11:22:33:44:55:66"

        first = tracker.observe_eof(sender_mac, 30, now=1.0)
        tracker.complete_cycle(sender_mac, now=2.0)

        second = tracker.observe_data(sender_mac, 31, now=3.0)

        self.assertEqual(first.cycle_id + 1, second.cycle_id)
        self.assertEqual(second.cycle_state, "ReceivingData")
        self.assertEqual(second.cycle_seq_num, 31)


if __name__ == "__main__":
    unittest.main()
