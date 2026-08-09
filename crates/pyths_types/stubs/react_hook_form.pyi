# Type stubs for `react-hook-form` — performant form library.

from typing import Any, Callable, Dict, List, Optional


class UseFormReturn:
    pass


class FieldValues:
    pass


def use_form(options: Optional[Any] = None) -> UseFormReturn:
    ...


def use_form_context() -> UseFormReturn:
    ...


def use_field_array(options: Any) -> Any:
    ...


def use_watch(options: Optional[Any] = None) -> Any:
    ...


def use_controller(options: Any) -> Any:
    ...


# Form components
class FormProvider:
    pass


class Controller:
    pass
