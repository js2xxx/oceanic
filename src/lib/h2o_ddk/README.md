# Architecture

The drivers and most devices are managed by devm in Oceanic, and drivers are dynamic libraries 
running on drvhost. The DDK crate should be included in the drivers to use its fundamental support
such as memory allocation and asynchronous task spawning.

The drivers are implemented fully asynchronous, so multi-task in one thread is supported. However,
if synchronous types are strongly preferred, by far only implementing raw APIs directly can work
well.

# Raw APIs

## `__h2o_ddk_enter`

To be called in drvhost to initialize the infrastructures in DDK crate and start the main task in 
the driver.

|       Name        | Implemented in | Called in |
|-------------------|----------------|-----------|
|`__h2o_ddk_enter`  |   solvent-ddk  |  drvhost  |
|`__h2o_ddk_exit`   |   solvent-ddk  |  drvhost  |

# VTable

TODO