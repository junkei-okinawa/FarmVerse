"""Per-sender cycle tracking for HASH / DATA / EOF frames."""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass, field
from typing import Dict, Optional

from .constants import FRAME_TYPE_DATA, FRAME_TYPE_EOF, FRAME_TYPE_HASH

logger = logging.getLogger(__name__)

TERMINAL_STATES = {"Completed", "TimedOut"}


@dataclass
class SenderCycleState:
    sender_mac: str
    cycle_id: int
    cycle_seq_num: Optional[int] = None
    cycle_state: str = "Idle"
    cycle_started_at: float = field(default_factory=time.monotonic)
    last_event_at: float = field(default_factory=time.monotonic)
    hash_received: bool = False
    eof_received: bool = False
    warning_emitted: bool = False
    last_frame_type: Optional[int] = None
    last_frame_seq_num: Optional[int] = None


class CycleTracker:
    """Track one active cycle per sender MAC."""

    def __init__(self) -> None:
        self._states: Dict[str, SenderCycleState] = {}
        self._cycle_counters: Dict[str, int] = {}

    def get_state(self, sender_mac: str) -> Optional[SenderCycleState]:
        return self._states.get(sender_mac)

    def observe_data(
        self, sender_mac: str, seq_num: Optional[int], now: Optional[float] = None
    ) -> SenderCycleState:
        state = self._ensure_active_state(sender_mac, seq_num, "ReceivingData", now)
        state.last_frame_type = FRAME_TYPE_DATA
        state.last_frame_seq_num = seq_num
        state.cycle_state = "ReceivingData"

        if state.cycle_seq_num is None and seq_num is not None:
            state.cycle_seq_num = seq_num

        if state.hash_received:
            logger.warning(
                "DATA received after HASH for %s (cycle_seq=%s, seq=%s)",
                sender_mac,
                state.cycle_seq_num,
                seq_num,
            )

        return state

    def observe_hash(
        self, sender_mac: str, seq_num: Optional[int], now: Optional[float] = None
    ) -> SenderCycleState:
        state = self._ensure_active_state(sender_mac, seq_num, "HashReceived", now)
        previous_cycle_seq = state.cycle_seq_num
        state.last_frame_type = FRAME_TYPE_HASH
        state.last_frame_seq_num = seq_num
        state.hash_received = True
        state.eof_received = False
        state.cycle_state = "HashReceived"

        if seq_num is not None:
            state.cycle_seq_num = seq_num

        if previous_cycle_seq is not None and previous_cycle_seq != state.cycle_seq_num:
            logger.debug(
                "Updated cycle sequence for %s after HASH: %s",
                sender_mac,
                state.cycle_seq_num,
            )

        return state

    def observe_eof(
        self, sender_mac: str, seq_num: Optional[int], now: Optional[float] = None
    ) -> SenderCycleState:
        state = self._ensure_active_state(sender_mac, seq_num, "EofReceived", now)
        state.last_frame_type = FRAME_TYPE_EOF
        state.last_frame_seq_num = seq_num
        state.eof_received = True
        state.cycle_state = "EofReceived"

        if state.cycle_seq_num is None and seq_num is not None:
            state.cycle_seq_num = seq_num

        if not state.hash_received and not state.warning_emitted:
            logger.warning(
                "EOF received but HASH was not received for %s (cycle_seq=%s)",
                sender_mac,
                state.cycle_seq_num,
            )
            state.warning_emitted = True

        return state

    def complete_cycle(
        self, sender_mac: str, now: Optional[float] = None
    ) -> Optional[SenderCycleState]:
        state = self._states.get(sender_mac)
        if state is None:
            return None

        if state.cycle_state in TERMINAL_STATES:
            return state

        state.cycle_state = "Completed"
        state.last_event_at = now if now is not None else time.monotonic()
        return state

    def prune_terminal_states(
        self, retention_seconds: float = 3600.0, now: Optional[float] = None
    ) -> int:
        """Remove terminal cycles that have been retained long enough."""
        current_time = now if now is not None else time.monotonic()
        removed = 0

        for sender_mac, state in list(self._states.items()):
            if state.cycle_state not in TERMINAL_STATES:
                continue

            if current_time - state.last_event_at < retention_seconds:
                continue

            del self._states[sender_mac]
            self._cycle_counters.pop(sender_mac, None)
            removed += 1

        return removed

    def mark_timeout(
        self, sender_mac: str, now: Optional[float] = None
    ) -> Optional[SenderCycleState]:
        state = self._states.get(sender_mac)
        if state is None:
            return None

        state.cycle_state = "TimedOut"
        state.last_event_at = now if now is not None else time.monotonic()
        return state

    def _ensure_active_state(
        self,
        sender_mac: str,
        seq_num: Optional[int],
        initial_state: str,
        now: Optional[float],
    ) -> SenderCycleState:
        state = self._states.get(sender_mac)
        if state is None or state.cycle_state in TERMINAL_STATES:
            state = self._create_state(sender_mac, seq_num, initial_state, now)
            self._states[sender_mac] = state
            if initial_state == "HashReceived":
                logger.warning(
                    "HASH received before DATA for %s (cycle_seq=%s, seq=%s)",
                    sender_mac,
                    state.cycle_seq_num,
                    seq_num,
                )
            elif initial_state == "EofReceived":
                logger.warning(
                    "EOF received before DATA/HASH for %s (cycle_seq=%s, seq=%s)",
                    sender_mac,
                    state.cycle_seq_num,
                    seq_num,
                )
                state.warning_emitted = True
            return state

        state.last_event_at = now if now is not None else time.monotonic()
        return state

    def _create_state(
        self,
        sender_mac: str,
        seq_num: Optional[int],
        initial_state: str,
        now: Optional[float],
    ) -> SenderCycleState:
        cycle_id = self._cycle_counters.get(sender_mac, 0) + 1
        self._cycle_counters[sender_mac] = cycle_id

        created_at = now if now is not None else time.monotonic()
        return SenderCycleState(
            sender_mac=sender_mac,
            cycle_id=cycle_id,
            cycle_seq_num=seq_num,
            cycle_state=initial_state,
            cycle_started_at=created_at,
            last_event_at=created_at,
            last_frame_seq_num=seq_num,
        )
