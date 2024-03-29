# The MIT License (MIT)
#
# Copyright (c) 2021 Jonah Yolles-Murphy (TG-Techie)
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in
# all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
# THE SOFTWARE.

import gc

from tg_gui_core import (
    State,
    Widget,
    Container,
    declarable,
    center,
    AttributeSpecifier,
)


class PageState(State):
    """
    Translates between the pages in
    """

    def value(self, reader):
        value = self._value
        if isinstance(value, AttributeSpecifier):
            page = self._value._resolve_specified_(reader)
            value = self._value = reader._nested_.index(page)
        return value

    def __get__(self, owner, ownertype):
        return owner._nested_[self.value(None)]

    def __set__(self, owner, value):
        assert (
            value in owner._nested_
        ), f"cannot switch to {owner} to page {value}, that page is not in its pages"
        self.update(None, owner._nested_.index(value))


@declarable
class Pages(Container):

    _full_refresh_ = True  # shhhh. this may be a hack, tbd

    def __init__(self, show=None, pages=None, _hot_rebuild_=None):
        super().__init__()

        if hasattr(self, "_hot_rebuild_") and _hot_rebuild_ is None:
            _hot_rebuild_ = self._hot_rebuild_
        if _hot_rebuild_ is None:
            _hot_rebuild_ = False

        # determin if is called, declared, or invalid
        was_declared = show is None and pages is None
        was_called = show is not None and pages is not None

        if was_declared:
            # find the decalred pages
            show, pages = self._scan_class_for_pages()
        elif was_called:
            self._decalrable_ = False
            assert isinstance(show, State), (
                f"when calling {type(self).__name__} the 'show' argumetn must "
                + "be a state or PageState object"
            )
            assert isinstance(pages, tuple) and len(pages), (
                f"when calling {type(self).__name__} the 'pages' argumetn must "
                + "a tuple with one or more widgets"
            )
        else:
            raise TypeError(
                f"{self} incorrectly inited, Pages must be called with a state "
                + "object and a tuple of pages or subclasses and declared "
                + "with @singleinstance"
            )

        self._state = show
        self._pages = pages
        self._current_page = None
        self._hot_rebuild = _hot_rebuild_

    def __len__(self):
        return len(self._nested_)

    def _on_nest_(self):
        for widget in self._pages:
            self._nest_(widget)
        self._pages = None

    def _form_(self, dim_spec):
        super(Container, self)._form_(dim_spec)
        size = self._size_
        for widget in self._nested_:
            widget._form_(size)

    def _place_(self, pos_spec):
        super(Container, self)._place_(pos_spec)
        for widget in self._nested_:
            widget._place_(center)

    def _build_(self):
        super(Container, self)._build_()

        self._current_page = current_page = self._nested_[self._state.value(self)]

        if self._hot_rebuild:
            current_page._build_()
        else:
            for widget in self._nested_:
                widget._build_()

        self._screen_.on_container_build(self)

        # print(f"registering {self} with {self._state}")
        self._state._register_handler_(self, self._switch_page)

    def _demolish_(self):
        self._state._deregister_handler_(self)
        super()._demolish_()

    def _show_(self):
        super(Container, self)._show_()
        self._current_page._show_()
        self._screen_.on_container_show(self)

    def _hide_(self):
        self._current_page._hide_()
        super(Container, self)._hide_()

    def _switch_page(self, index):
        # print(f"{self}._switch_page{index}")
        if self.isshowing():
            self._current_page._hide_()
        if self._hot_rebuild:
            print("before demo", gc.mem_free(), end=" ")
            self._current_page._demolish_()
            gc.collect()
            print("after demo", gc.mem_free(), end=" ")

        self._screen_.on_container_hide(self)
        self._current_page = to_show = self._nested_[index]
        self._screen_.on_container_show(self)

        if self._hot_rebuild:
            print("before build", gc.mem_free(), end=" ")
            to_show._build_()
            gc.collect()
            print("after build", gc.mem_free(), end=" ")
            print()
        if self.isshowing():
            to_show._show_()

    @classmethod
    def _scan_class_for_pages(cls):

        # see if the user added a page class attribute(attr)
        if hasattr(cls, "page"):
            state = cls.page
        else:
            raise TypeError(
                f"no page stae object declared in {cls}, add `page = State(0)` "
                + "to  the body"
            )

        attrs = [getattr(cls, name) for name in dir(cls)]
        found_pages = []
        for attr in attrs:
            if isinstance(attr, Widget):
                found_pages.append(attr)
            elif isinstance(attr, type) and issubclass(attr, Widget):
                raise TypeError(
                    f"type {attr} found in the body of {cls}, this is not a "
                    + "valid declaration. did you mean to decorate it "
                    + "with @singleinstance"
                )
            else:
                pass
        found_pages.sort(key=lambda widget: widget._id_)
        return state, tuple(found_pages)
