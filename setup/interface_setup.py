from hardware import touchscreen, display

from tg_gui_core import *
from tg_gui_core.base import Defaults, Palette, Palettes

from tg_gui_std.all import *
from tg_gui_platform.event_loops import SinglePointEventLoop
from tg_gui_platform.root_wrapper import DisplayioRootWrapper, DisplayioScreen

screen = DisplayioScreen(
    layout_class=layout_class.landscape,
    display=display,
    min_size=50,
    default=Defaults(
        margin=5,
        radius=20,  # self.height // 2,
        font_size=2,
        _fill_color_=0x20609F,  # 0x20609f,
        _selected_fill_=0x7FFFFF,
        _text_color_=0xFFFFFF,
        _selected_text_=0x000000,
    ),
    palettes=Palettes(
        primary=Palette(
            fill_color=0x20609F,  # 0x20609f,
            text_color=color.white,
            selected_fill=0x7FFFFF,
            selected_text=color.black,
            backfill=color.gray,
        ),
        secondary=Palette(
            fill_color=color.black,
            text_color=color.lightgray,
            selected_fill=color.gray,
            selected_text=color.black,
            backfill=color.red,
        ),
    ),
)

default, palette = screen.default, screen.palettes


def get_touch_coord():
    ## TODO: correct scaling and rotations
    global touchscreen
    if touchscreen.touched:
        point_dict = touchscreen.touches[0]
        raw_x = point_dict["x"]
        raw_y = point_dict["y"]
        y = int(raw_x)
        x = int(320 - raw_y)
        return (x, y)
    else:
        return None


event_loop = SinglePointEventLoop(screen=screen, poll_coord=get_touch_coord)

display.refresh()
