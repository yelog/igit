import unittest
from types import SimpleNamespace
from unittest.mock import Mock

from igit_resize import handle_result


def fixture(neighbors, layout="splits"):
    tab = SimpleNamespace(
        current_layout=SimpleNamespace(name=layout, neighbors_for_window=Mock(return_value=neighbors)),
        windows=[], resize_window_by=Mock(), active_window=SimpleNamespace(id=99),
    )
    window = SimpleNamespace(id=7, tabref=lambda: tab)
    boss = SimpleNamespace(window_id_map={7: window}, active_tab=Mock())
    return boss, window, tab


class ResizeTests(unittest.TestCase):
    def test_moves_boundary_from_both_sides_on_each_axis(self):
        for direction, horizontal, positive in [
            ("left", True, False), ("right", True, True),
            ("up", False, False), ("down", False, True),
        ]:
            for before, after in [(True, False), (False, True), (True, True)]:
                with self.subTest(direction=direction, before=before, after=after):
                    neighbors = {"left" if horizontal else "top": [1] if before else [],
                                 "right" if horizontal else "bottom": [2] if after else []}
                    boss, window, tab = fixture(neighbors)
                    handle_result(["igit_resize.py", direction, "3"], None, 7, boss)
                    tab.resize_window_by.assert_called_once_with(7, 3 if positive == after else -3, horizontal)
                    tab.current_layout.neighbors_for_window.assert_called_once_with(window, tab.windows)
                    self.assertEqual(tab.active_window.id, 99)
                    self.assertFalse(boss.active_tab.mock_calls)

    def test_single_window_wrong_axis_stack_or_closed_window_do_nothing(self):
        for neighbors, layout in [({}, "splits"), ({"top": [1]}, "splits"), ({"right": [1]}, "stack")]:
            boss, _, tab = fixture(neighbors, layout)
            handle_result(["igit_resize.py", "right", "3"], None, 7, boss)
            tab.resize_window_by.assert_not_called()
        boss, window, tab = fixture({"right": [1]})
        handle_result(["igit_resize.py", "right", "3"], None, 8, boss)
        window.tabref = lambda: None
        handle_result(["igit_resize.py", "right", "3"], None, 7, boss)
        tab.resize_window_by.assert_not_called()

    def test_invalid_requests_cannot_resize(self):
        boss, _, tab = fixture({"right": [1]})
        for arguments in [[], ["right"], ["right", "3", "extra"], ["bad", "3"],
                          ["right", "bad"], ["right", "0"], ["right", "-3"]]:
            handle_result(["igit_resize.py"] + arguments, None, 7, boss)
        tab.resize_window_by.assert_not_called()


if __name__ == "__main__":
    unittest.main()
