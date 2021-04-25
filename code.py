import sys
import oh_shit
import hardware

# show splash while loading watch_gui
from boot_options import splash_screen

# init the gui system
from boot_options import main_interface


# clean up splash
sys.modules.pop(splash_screen.__name__)
del splash_screen

if __name__ == "__main__":
    main_interface.run()
