# To import all the connectors in the package, use:
#   from connectors import *

"""
This module provides the base class for connectors and imports specific connectors.
"""

from . import openai
# TODO: Add `scrungly` and `local` connectors

from abc import ABC, abstractmethod

# Abstract base class for connectors (analogous to interfaces in Java/C#)
class BaseConnector(ABC):
    @abstractmethod
    def __init__(self, *args, **kwargs):
        """
        Initialize the connector and open any relevant API connections.
        """
        pass

    @abstractmethod
    def completion(self, *args, **kwargs):
        """
        Generate a completion from the model.
        """
        pass

    @abstractmethod
    def close(self):
        """
        Close any open API connections.
        """
        pass

    @abstractmethod
    def __del__(self):
        """
        Attempts to close any open API connections when the object is deleted.
        """
        try:
            self.close()
        except:
            pass