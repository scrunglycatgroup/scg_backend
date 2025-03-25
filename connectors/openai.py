from .__init__ import BaseConnector
from common import *

from openai import OpenAI, NotGiven
from typing import Optional, Mapping, Union
import httpx
from httpx import Timeout

NOT_GIVEN = NotGiven()

class OpenAIConnector(BaseConnector):
    __model_name : str
    __client : OpenAI

    def __init__(
            self,
            *,
            model_name:str,
            **kwargs):

        self.__model_name__ = model_name
        
        if self.__model_name == "undefined":
            raise ValueError("model_name must be defined")

        self.__client = OpenAI(
            api_key=kwargs.get("api_key", None),
            organization=kwargs.get("organization", None),
            project=kwargs.get("project", None),
            base_url=kwargs.get("base_url", None),
            websocket_base_url=kwargs.get("websocket_base_url", None),
            timeout=kwargs.get("timeout", NOT_GIVEN),
            max_retries=kwargs.get("max_retries", 2),
            default_headers=kwargs.get("default_headers", None),
            default_query=kwargs.get("default_query", None),
            http_client=kwargs.get("http_client", None),
            _strict_response_validation=kwargs.get("_strict_response_validation", False)
        )
