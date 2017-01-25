// Copyright 2015-2017Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

import React, { Component, PropTypes } from 'react';
import CircularProgress from 'material-ui/CircularProgress';

import styles from './loading.css';

export default class Loading extends Component {
  static propTypes = {
    className: PropTypes.string,
    size: PropTypes.number
  };

  static defaultProps = {
    className: '',
    size: 2
  };

  render () {
    const { className, size } = this.props;
    const computedSize = size * 60;

    return (
      <div className={ [ styles.loading, className ].join(' ') }>
        <CircularProgress size={ computedSize } />
      </div>
    );
  }
}
