from tg_gui_std.all import *
import hardware


@singleinstance
class Tab1(Layout):
    body = HSplit(
        VSplit(
            Button(text="1", press=lambda: None),
            Button(text="2", press=lambda: None),
        ),
        VSplit(
            Button(text="3", press=lambda: None),
            Button(text="4", press=lambda: None),
        ),
    )

    def _any_(self):
        self.body(center, self.dims)
