import displayio
import gc
import time
import sys

import oh_shit
import hardware

from tg_gui_core import *
from tg_gui_platform.root_wrapper import DisplayioRootWrapper
from tg_gui_std.all import *

from setup.interface_setup import screen, display, event_loop

from setup.tab_1 import Tab1


@DisplayioRootWrapper(screen=screen, display=display, size=(320, 240))
class WatchRoot(Layout):
    active_tab = State(0)

    switcher = HSplit(
        Button(text="Tab 1", press=self.open_tab(0), radius=4),
        Button(text="Tab 2", press=self.open_tab(1), radius=4),
        Button(text="Tab 3", press=self.open_tab(2), radius=4),
    )
    tabs = Pages(
        pages=(Tab1, Rect(fill=color.green), Rect(fill=color.blue)),
        show=active_tab,
    )

    def open_tab(self, index):
        self.active_tab = index

    def _any_(self):
        switcher = self.switcher(top, (self.width, self.height // 6))
        self.tabs(below(switcher), (self.width, self.height - switcher.height))


WatchRoot._superior_._std_startup_()
gc.collect()
print(gc.mem_free())


def run():
    global WatchRoot, screen
    gc.collect()
    try:
        print(gc.mem_free())
        while True:
            gc.collect()
            event_loop.loop()
            display.refresh()

    except Exception as err:
        gc.collect()
        del WatchRoot
        del screen
        sys.print_exception(err)
        display.show(None)
        display.refresh()
        gc.collect()
        if isinstance(err, MemoryError):
            oh_shit.reset_countdown(10, err)
        else:
            oh_shit.reset_countdown(30, err)
