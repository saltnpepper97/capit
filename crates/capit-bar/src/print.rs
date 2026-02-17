// Author: Dustin Pilgrim
// License: MIT

use capit_ipc::Response;

pub fn print_response(resp: Response) {
    eprintln!("{resp:?}");
}
