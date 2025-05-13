from pydantic import BaseModel, Field
from typing import Optional, List, Dict, TypedDict

class Connector(TypedDict):
    connector: str
    model_name: str
    model_tag: Optional[str] = "undefined"
    arguments: Optional[List[Dict[str, str]]] = []

class ModelRequest(BaseModel):
    content: str
    language: Optional[str] = "undefined"
    language_version: Optional[str] = "undefined"
    purpose: str
    extra_data: Optional[Dict[str, str]] = {}
    connector: Connector = Field(default_factory=lambda: {
        "connector": "undefined",
        "model_name": "undefined",
        "model_tag": "undefined",
        "arguments": []
    })

class ModelResponse(BaseModel):
    content: str
    status_code: Optional[int] = 200
    flags: Optional[List[str]] = []

if __name__ == "__main__":
    #TODO: Update tests for new ModelRequest format
    
    # test the ModelRequest class
    test = ModelRequest(content="Hello, World!")
    print(test)

    # test the ModelRequest class with language and language_version
    test = ModelRequest(content="Hello, World!", language="Python", language_version="3.8")
    print(test)

    # test the Connector class
    test = Connector(connector="undefined", model_name="undefined", model_tag="undefined", arguments=[])
    print(test)

    # test the Connector class with arguments
    test = Connector(connector="undefined", model_name="undefined", model_tag="undefined", arguments=[{"key": "value"}])
    print(test)

    # test the Connector class with multiple arguments
    test = Connector(connector="undefined", model_name="undefined", model_tag="undefined", arguments=[{"key": "value"}, {"key": "value"}])
    print(test)

    # test the ModelRequest class with connector
    test = ModelRequest(content="Hello, World!", connector=Connector(connector="undefined", model_name="undefined", model_tag="undefined", arguments=[]))
    print(test)

    # test the ModelResponse class
    test = ModelResponse(content="Hello, World!")
    print(test)
